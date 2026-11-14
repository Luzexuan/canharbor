// SPDX-License-Identifier: AGPL-3.0-or-later
// The browser decoder against the same data as crates/canharbor-dbc: the
// cantools reference vectors and the edge cases of the Rust tests.
import { test, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { DbcDatabase, DbcError, hexBytes, signalText } from "../src/dbc";

const reference = (name: string) =>
  readFileSync(resolve("../crates/canharbor-dbc/tests", name));
const ascii = (text: string) => Uint8Array.from(text, (c) => c.charCodeAt(0));
const parse = (text: string) => DbcDatabase.parse(ascii(text));
const kind = (text: string | Uint8Array) => {
  try {
    DbcDatabase.parse(typeof text === "string" ? ascii(text) : text);
  } catch (error) {
    if (error instanceof DbcError) return error.kind;
    throw error;
  }
  return "ok";
};
const vectors = (name: string) =>
  reference(name)
    .toString("utf8")
    .split("\n")
    .filter((line) => line !== "")
    .map((line) => {
      const [id, extended, payload] = line.split(" ", 3);
      return {
        line,
        id: Number.parseInt(id, 16),
        extended: extended === "1",
        data: hexBytes(payload),
        pairs: line
          .slice(id.length + extended.length + payload.length + 3)
          .split(";")
          .map((pair) => {
            const at = pair.indexOf("=");
            return [pair.slice(0, at), pair.slice(at + 1)] as const;
          }),
      };
    });

test("decoding matches the reference implementation", () => {
  const database = DbcDatabase.parse(reference("reference.dbc"));
  // The comment in the file contains text shaped like a message definition.
  expect(database.decode(999, false, new Uint8Array(8))).toBeNull();
  let compared = 0;
  for (const { line, id, extended, data, pairs } of vectors("reference.txt")) {
    const decoded = database.decode(id, extended, data);
    if (!decoded) throw new Error(`no message for ${line}`);
    expect(decoded.truncated, line).toBe(0);
    expect(decoded.signals.length, line).toBe(pairs.length);
    for (const [name, text] of pairs) {
      const signal = decoded.signals.find((s) => s.name === name);
      if (!signal) throw new Error(`${name} missing in ${line}`);
      const value = Number(text);
      const same = Number.isNaN(value)
        ? Number.isNaN(signal.number)
        : !Number.isFinite(value)
          ? signal.number === value
          : Math.abs(signal.number - value) <=
            Math.max(Math.abs(value), 1) * 1e-12;
      if (!same)
        throw new Error(`${name}: ${signal.number} != ${value} in ${line}`);
      compared++;
    }
  }
  expect(compared).toBeGreaterThan(2500);
});

test("value table names match the reference implementation", () => {
  const database = DbcDatabase.parse(reference("reference.dbc"));
  let named = 0;
  for (const { line, id, extended, data, pairs } of vectors(
    "reference_labels.txt",
  )) {
    const decoded = database.decode(id, extended, data)!;
    for (const [name, label] of pairs) {
      const signal = decoded.signals.find((s) => s.name === name)!;
      expect(signal.label ?? "-", `${name} in ${line}`).toBe(label);
      if (signal.label !== null) named++;
    }
  }
  expect(named).toBeGreaterThan(50);
});

test("unscaled integers stay exact and units are kept", () => {
  const database = parse(
    'BO_ 291 Engine: 8 ECU\n SG_ Counter : 0|64@1+ (1,0) [0|0] "" X\n' +
      'BO_ 2147483939 Ext: 2 ECU\n SG_ Temp : 7|16@0- (0.5,-40) [0|0] "degC" X\n',
  );
  expect(database.messages).toBe(2);
  const ones = new Uint8Array(8).fill(0xff);
  const engine = database.decode(0x123, false, ones)!;
  expect(engine.name).toBe("Engine");
  expect(engine.signals[0].value).toBe("18446744073709551615");
  // The same number is a different message when the identifier width differs.
  expect(database.decode(0x123, true, ones)).not.toBeNull();
  expect(database.decode(0x124, false, ones)).toBeNull();
  const ext = database.decode(0x123, true, Uint8Array.of(0xff, 0xfe))!;
  expect(ext.signals[0].value).toBe("-41");
  expect(signalText(ext)).toBe("Ext: Temp=-41 degC");
  // Signed 64-bit values keep every digit as well.
  const signed = parse('BO_ 1 M: 8 E\n SG_ S : 0|64@1- (1,0) [0|0] "" X\n');
  expect(
    signed.decode(1, false, Uint8Array.of(0, 0, 0, 0, 0, 0, 0, 0x80))!
      .signals[0].value,
  ).toBe("-9223372036854775808");
});

test("short payloads and unknown selectors do not produce values", () => {
  const database = parse(
    'BO_ 1 M: 8 ECU\n SG_ Sel M : 0|8@1+ (1,0) [0|0] "" X\n' +
      ' SG_ A m0 : 8|8@1+ (1,0) [0|0] "" X\n SG_ B m1 : 56|8@1+ (1,0) [0|0] "" X\n',
  );
  const names = (...data: number[]) => {
    const decoded = database.decode(1, false, Uint8Array.from(data))!;
    return [decoded.signals.map((s) => s.name), decoded.truncated];
  };
  expect(names(0, 7, 0, 0, 0, 0, 0, 9)).toEqual([["Sel", "A"], 0]);
  expect(names(1, 7, 0, 0, 0, 0, 0, 9)).toEqual([["Sel", "B"], 0]);
  expect(names(5, 7, 0, 0, 0, 0, 0, 9)).toEqual([["Sel"], 0]);
  // B lies beyond a two-byte payload: reported as truncated, not as zero.
  expect(names(1, 7)).toEqual([["Sel"], 1]);
  expect(names()).toEqual([[], 1]);
});

test("records are recognised with any whitespace and escaped quotes", () => {
  const database = parse(
    'CM_ "C:\\\\logs\\\\";\nBO_\t291\tEngine :\t8 ECU\n\tSG_\tRatio\t:\t0|32@1+\t(1,0)\t[0|0]\t""\tX\n' +
      "SIG_VALTYPE_\t291\tRatio\t:\t1;\n",
  );
  const decoded = database.decode(
    0x123,
    false,
    Uint8Array.of(0, 0, 0x80, 0x3f),
  )!;
  expect(decoded.name).toBe("Engine");
  expect(decoded.signals[0].value).toBe("1");
  // An escaped quote keeps a string open; the record-shaped text inside is skipped.
  const quoted = parse(
    'CM_ "say \\"hi\\"\nBO_ 5 Fake: 8 ECU\n";\nBO_ 6 Real: 8 ECU\n',
  );
  expect(quoted.decode(5, false, new Uint8Array())).toBeNull();
  expect(quoted.decode(6, false, new Uint8Array())).not.toBeNull();
  expect(kind("BO_ 1 M: 8 E\nSG_MUL_VAL_\t1 A B 1-1;\n")).toBe("unsupported");
  // Windows line endings.
  const crlf = parse(
    'BO_ 7 M: 8 ECU\r\n SG_ S : 0|8@1+ (1,0) [0|0] "V" X\r\nVAL_ 7 S 9 "Nine" ;\r\n',
  );
  expect(signalText(crlf.decode(7, false, Uint8Array.of(9))!)).toBe(
    "M: S=9 (Nine) V",
  );
});

test("namespace entries are not records", () => {
  const signal = 'BO_ 7 M: 8 ECU\n SG_ S : 0|8@1+ (1,0) [0|0] "" X\n';
  const database = parse(
    'VERSION ""\n\nNS_ :\n\tCM_\n\tSIG_VALTYPE_\n\tSG_MUL_VAL_\n\tBO_TX_BU_\nBS_:\n\nBU_: ECU\n' +
      signal,
  );
  expect(database.decode(7, false, Uint8Array.of(9))!.signals[0].value).toBe(
    "9",
  );
  // Blank and whitespace-only lines inside the header do not end it.
  expect(
    parse(
      "NS_ :\n\n\tSIG_VALTYPE_\n \t \n\tSG_MUL_VAL_\n\nBS_:\n" + signal,
    ).decode(7, false, Uint8Array.of(9)),
  ).not.toBeNull();
  // Outside the header a real extended-multiplexing record is still refused.
  expect(
    kind("NS_ :\n\tSG_MUL_VAL_\n\nBO_ 1 M: 8 E\nSG_MUL_VAL_ 1 A B 1-1;\n"),
  ).toBe("unsupported");
});

test("units keep their characters in UTF-8 and Windows-1252 files", () => {
  const signal = (unit: number[]) =>
    Uint8Array.from([
      ...ascii('BO_ 1 M: 8 E\n SG_ S : 0|8@1+ (1,0) [0|0] "'),
      ...unit,
      ...ascii('" X\n'),
    ]);
  const unit = (bytes: number[]) =>
    DbcDatabase.parse(signal(bytes)).decode(1, false, Uint8Array.of(0))!
      .signals[0].unit;
  const codes = (text: string) => Array.from(ascii(text));
  // Windows-1252: degree sign, micro sign, euro sign, an undefined byte.
  expect(unit([0xb0, 0x43])).toBe("\u00b0C");
  expect(unit([0xb5, 0x73])).toBe("\u00b5s");
  expect(unit([0x80])).toBe("\u20ac");
  expect(unit([0x81])).toBe("\u0081");
  // Escaped characters inside the unit: an inch mark and a backslash.
  expect(unit(codes('\\"'))).toBe('"');
  expect(unit(codes("a\\\\b"))).toBe("a\\b");
  expect(kind(signal(codes("open\\")))).toBe("line");
  // The same characters in UTF-8 are not decoded a second time.
  expect(unit(Array.from(new TextEncoder().encode("\u00b0C \u20ac")))).toBe(
    "\u00b0C \u20ac",
  );
});

test("value tables are optional and strictly parsed", () => {
  const base =
    'BO_ 1 M: 8 E\n SG_ S : 0|8@1- (1,0) [0|0] "" X\n SG_ F : 8|32@1+ (1,0) [0|0] "" X\nSIG_VALTYPE_ 1 F : 1;\n';
  const data = Uint8Array.of(0xff, 0, 0, 0x80, 0x3f);
  const label = (tables: string) =>
    parse(base + tables)
      .decode(1, false, data)!
      .signals.map((s) => s.label);
  // Signed raw values and tabs between fields.
  expect(label('VAL_\t1 S -1 "Invalid"\t0 "Off";\n')[0]).toBe("Invalid");
  // Runs of spaces and tabs between any two fields.
  expect(label('VAL_  1 \t S   -1  "Invalid"   0\t"Off"  ;\n')[0]).toBe(
    "Invalid",
  );
  // The whole unsigned 64-bit range and the most negative signed value.
  const wide = parse(
    'BO_ 2 W: 8 E\n SG_ U : 0|64@1+ (1,0) [0|0] "" X\nBO_ 3 N: 8 E\n SG_ I : 0|64@1- (1,0) [0|0] "" X\n' +
      'VAL_ 2 U 18446744073709551615 "All ones" 9223372036854775808 "Top bit" ;\n' +
      'VAL_ 3 I -9223372036854775808 "Minimum" -1 "Minus one" ;\n',
  );
  const name = (id: number, ...bytes: number[]) =>
    wide.decode(id, false, Uint8Array.from(bytes))!.signals[0].label;
  const ones = Array<number>(8).fill(0xff);
  const top = [0, 0, 0, 0, 0, 0, 0, 0x80];
  expect(name(2, ...ones)).toBe("All ones");
  expect(name(2, ...top)).toBe("Top bit");
  expect(name(3, ...top)).toBe("Minimum");
  expect(name(3, ...ones)).toBe("Minus one");
  // A float signal has no integer raw value to name.
  expect(label('VAL_ 1 F 1 "One" ;\n')[1]).toBeNull();
  // Tables for unknown signals, other messages and environment variables are ignored.
  expect(
    label('VAL_ 1 Other 0 "x" ;\nVAL_ 2 S -1 "x" ;\nVAL_ Env 0 "x" ;\n'),
  ).toEqual([null, null]);
  // A record that continues on the next line is left out whole.
  for (const continued of [
    'VAL_ 1 S -1 "two\nlines" ;\n',
    'VAL_ 1 S -1\n "Invalid" 0 "Off" ;\n',
    'VAL_ 1 S -1 "Invalid"\n 0 "Off" ;\n',
    'VAL_ 1 S -1 "Invalid" 0 "Off"\n',
  ])
    expect(label(continued), continued).toEqual([null, null]);
  for (const malformed of [
    'VAL_ 1 S x "a" ;\n',
    "VAL_ 1 S 1 a ;\n",
    "VAL_ 1 S 1 ;\n",
    'VAL_ 1 S 18446744073709551616 "a" ;\n',
    'VAL_ 1 S -9223372036854775809 "a" ;\n',
  ])
    expect(kind(base + malformed), malformed).toBe("line");
});

test("malformed and unsupported files are rejected", () => {
  for (const [text, expected] of [
    ["BO_ x M: 8 E\n", "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|0@1+ (1,0) [0|0] "" X\n', "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|65@1+ (1,0) [0|0] "" X\n', "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|8@2+ (1,0) [0|0] "" X\n', "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|8@1+ (nan,0) [0|0] "" X\n', "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|8@1+ (0x10,0) [0|0] "" X\n', "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|8@1+ (,0) [0|0] "" X\n', "line"],
    ['BO_ 1 M: 8 E\n SG_ S : 0|8@1+ (1e999,0) [0|0] "" X\n', "line"],
    ["BO_ 1 M: 8 E\n SG_ S : 0|8@1+ (1,0) [0|0] X\n", "line"],
    [
      'BO_ 1 M: 8 E\n SG_ S : 0|16@1+ (1,0) [0|0] "" X\nSIG_VALTYPE_ 1 S : 1;\n',
      "line",
    ],
    ['BO_ 1 M: 8 E\n SG_ S m1M : 0|8@1+ (1,0) [0|0] "" X\n', "unsupported"],
    ["BO_ 1 M: 8 E\nSG_MUL_VAL_ 1 A B 1-1;\n", "unsupported"],
    [
      'BO_ 1 M: 8 E\n SG_ A M : 0|8@1+ (1,0) [0|0] "" X\n SG_ B M : 8|8@1+ (1,0) [0|0] "" X\n',
      "unsupported",
    ],
  ])
    expect(kind(text), text).toBe(expected);
  expect(parse("").decode(1, false, new Uint8Array())).toBeNull();
  expect(kind(new Uint8Array(16 * 1024 * 1024 + 1))).toBe("too_large");
});
