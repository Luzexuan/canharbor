// SPDX-License-Identifier: AGPL-3.0-or-later
// Signal decoding from a DBC subset, for display only. This follows
// crates/canharbor-dbc line by line: the same records, the same limits and the
// same refusals. tests/dbc.spec.ts runs both the cantools reference vectors and
// the edge cases of the Rust tests against it. Change the two together.

const MAX_BYTES = 16 * 1024 * 1024;
const MAX_SIGNALS = 200_000;
const U64_MAX = (1n << 64n) - 1n;
const I64_MIN = -(1n << 63n);

export class DbcError extends Error {
  constructor(
    readonly kind: "too_large" | "line" | "unsupported",
    readonly line: number,
    readonly reason: string,
  ) {
    super(
      kind === "too_large"
        ? "DBC file exceeds the 16 MiB or signal-count limit"
        : `DBC line ${line}: ${kind === "unsupported" ? "unsupported: " : ""}${reason}`,
    );
  }
}

type Kind = "unsigned" | "signed" | "float";
// null: not multiplexed; "selector": the `M` signal; a bigint: shown only
// when the selector has this value.
type Mux = null | "selector" | bigint;
type Signal = {
  name: string;
  start: number;
  length: number;
  littleEndian: boolean;
  kind: Kind;
  factor: number;
  offset: number;
  unit: string;
  mux: Mux;
  // Names for raw values, from a VAL_ record, in file order.
  labels: [bigint, string][];
};
type Message = { name: string; signals: Signal[] };

export type DecodedSignal = {
  name: string;
  /** Exact decimal digits for an unscaled integer, otherwise the number. */
  value: string;
  /** The same value as a number; unscaled 64-bit integers may be rounded. */
  number: number;
  unit: string;
  /** The name the database gives this raw value, if any. */
  label: string | null;
};
export type DecodedMessage = {
  name: string;
  signals: DecodedSignal[];
  /** Signals whose bits lie beyond the received payload. */
  truncated: number;
};

// Windows-1252 differs from Latin-1 only in 0x80..=0x9F.
const WINDOWS_1252 = [
  0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030,
  0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d,
  0x2022, 0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e,
  0x178,
];
const WHITESPACE = /\s/;
/** Decimal digits with an optional plus sign, at most `max`. */
function unsigned(text: string | undefined, max: bigint): bigint | null {
  if (text === undefined || !/^\+?[0-9]+$/.test(text)) return null;
  const value = BigInt(text);
  return value <= max ? value : null;
}
function finiteOrNot(text: string): number | null {
  const trimmed = text.trim();
  if (/^[+-]?(inf|infinity|nan)$/i.test(trimmed)) return Number.NaN;
  if (!/^[+-]?([0-9]+\.?[0-9]*|\.[0-9]+)([eE][+-]?[0-9]+)?$/.test(trimmed))
    return null;
  return Number(trimmed);
}
/**
 * The contents of the first quoted string in `text` and the text after it. A
 * backslash escapes the character after it.
 */
function takeQuoted(text: string): [string, string] | null {
  const start = text.indexOf('"');
  if (start < 0) return null;
  let value = "";
  const characters = Array.from(text.slice(start + 1));
  let consumed = start + 1;
  for (let i = 0; i < characters.length; i++) {
    const character = characters[i];
    consumed += character.length;
    if (character === '"') return [value, text.slice(consumed)];
    if (character === "\\") {
      const next = characters[++i];
      if (next === undefined) return null;
      consumed += next.length;
      value += next;
    } else value += character;
  }
  return null;
}
/** The next whitespace-delimited field and the text after it. */
function field(text: string): [string, string] | null {
  const trimmed = text.trimStart();
  const end = trimmed.search(WHITESPACE);
  return end < 0 ? null : [trimmed.slice(0, end), trimmed.slice(end)];
}
function splitOnce(text: string, separator: string): [string, string] | null {
  const at = text.indexOf(separator);
  return at < 0 ? null : [text.slice(0, at), text.slice(at + separator.length)];
}
function words(text: string): string[] {
  return text.split(/\s+/).filter((word) => word !== "");
}

function rawBits(data: Uint8Array, signal: Signal): bigint | null {
  let raw = 0n;
  if (signal.littleEndian) {
    // The start bit is the least significant bit; bit n is bit n%8 of byte n/8.
    for (let i = 0; i < signal.length; i++) {
      const bit = signal.start + i;
      const byte = data[bit >> 3];
      if (byte === undefined) return null;
      raw |= BigInt((byte >> (bit & 7)) & 1) << BigInt(i);
    }
  } else {
    // The start bit is the most significant bit. Within a byte the next bit
    // is one lower; after bit 0 it continues at bit 7 of the next byte.
    let bit = signal.start;
    for (let i = 0; i < signal.length; i++) {
      const byte = data[bit >> 3];
      if (byte === undefined) return null;
      raw = (raw << 1n) | BigInt((byte >> (bit & 7)) & 1);
      bit = bit % 8 === 0 ? bit + 15 : bit - 1;
    }
  }
  return raw;
}
/** The raw value as the integer a value table refers to. */
function rawInteger(raw: bigint, signal: Signal): bigint | null {
  if (signal.kind === "float") return null;
  return signal.kind === "signed" ? BigInt.asIntN(signal.length, raw) : raw;
}
function physical(raw: bigint, signal: Signal): [string, number] {
  const scaled = signal.factor !== 1 || signal.offset !== 0;
  if (signal.kind === "float") {
    const view = new DataView(new ArrayBuffer(8));
    let value: number;
    if (signal.length === 32) {
      view.setUint32(0, Number(raw));
      value = view.getFloat32(0);
    } else {
      view.setBigUint64(0, raw);
      value = view.getFloat64(0);
    }
    value = value * signal.factor + signal.offset;
    return [String(value), value];
  }
  const integer = rawInteger(raw, signal) as bigint;
  if (scaled) {
    const value = Number(integer) * signal.factor + signal.offset;
    return [String(value), value];
  }
  return [integer.toString(), Number(integer)];
}

export class DbcDatabase {
  private constructor(private readonly table: Map<string, Message>) {}

  get messages(): number {
    return this.table.size;
  }

  static parse(bytes: Uint8Array): DbcDatabase {
    if (bytes.length > MAX_BYTES) throw new DbcError("too_large", 0, "");
    // Names and numbers are ASCII, but units and comments are often
    // Windows-1252. Valid UTF-8 is taken as such; anything else is read as
    // Windows-1252 so that a unit like "°C" is not turned into U+FFFD.
    let text: string;
    try {
      text = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(
        bytes,
      );
    } catch {
      // Spelled out so that every runtime agrees on 0x80..=0x9F, including
      // the five bytes Windows-1252 leaves undefined.
      const characters: string[] = [];
      for (let at = 0; at < bytes.length; at += 8192)
        characters.push(
          String.fromCharCode(
            ...Array.from(bytes.subarray(at, at + 8192), (byte) =>
              byte >= 0x80 && byte <= 0x9f ? WINDOWS_1252[byte - 0x80] : byte,
            ),
          ),
        );
      text = characters.join("");
    }
    const table = new Map<string, Message>();
    const key = (raw: bigint) =>
      `${raw & 0x1fff_ffffn}:${(raw & 0x8000_0000n) !== 0n}`;
    let current: string | null = null;
    const floats: [number, string, string, bigint][] = [];
    const tables: [string, string, [bigint, string][]][] = [];
    let signals = 0;
    // Comments and attribute strings may span lines and contain text that
    // looks like a record; skip everything inside an open string.
    let inString = false;
    // The `NS_ :` header lists record keywords, one bare word per line.
    let inNamespace = false;
    const lines = text.split("\n");
    if (lines[lines.length - 1] === "") lines.pop();
    for (let index = 0; index < lines.length; index++) {
      const number = index + 1;
      const whole = lines[index].endsWith("\r")
        ? lines[index].slice(0, -1)
        : lines[index];
      // A quote is escaped only by an odd number of preceding backslashes.
      let quotes = 0;
      let backslashes = 0;
      for (let i = 0; i < whole.length; i++) {
        const character = whole[i];
        if (character === '"' && backslashes % 2 === 0) quotes++;
        backslashes = character === "\\" ? backslashes + 1 : 0;
      }
      const continued = inString;
      if (quotes % 2 !== 0) inString = !inString;
      if (continued) continue;
      const bad = (reason: string) => new DbcError("line", number, reason);
      const unsupported = (reason: string) =>
        new DbcError("unsupported", number, reason);
      // Records are a keyword followed by any whitespace, not only a space.
      const line = whole.trim();
      const keyword = words(line)[0] ?? "";
      const rest = line.slice(keyword.length).trimStart();
      // The header ends at the first line that is more than one word. Blank
      // lines inside it do not end it.
      if (inNamespace && rest === "") continue;
      inNamespace = keyword === "NS_" || keyword === "NS_:";
      if (inNamespace) continue;
      if (keyword === "BO_") {
        const fields = words(rest);
        const raw = unsigned(fields[0], 0xffff_ffffn);
        if (raw === null) throw bad("message identifier");
        // The colon may follow the name directly or after whitespace.
        let name = fields[1];
        if (name?.endsWith(":")) name = name.slice(0, -1);
        if (!name || name.includes(":")) throw bad("message name");
        const extended = (raw & 0x8000_0000n) !== 0n;
        const id = raw & 0x1fff_ffffn;
        if ((raw & 0x6000_0000n) !== 0n || (!extended && id > 0x7ffn)) {
          // Includes the 0xC0000000 pseudo-message for unassigned signals.
          current = null;
          continue;
        }
        current = key(raw);
        table.set(current, { name, signals: [] });
      } else if (keyword === "SG_") {
        if (current === null) continue;
        if (++signals > MAX_SIGNALS) throw new DbcError("too_large", 0, "");
        const definition = splitOnce(rest, ":");
        if (!definition) throw bad("signal definition");
        const head = words(definition[0]);
        const name = head[0];
        if (!name) throw bad("signal name");
        let mux: Mux = null;
        if (head[1] === "M") mux = "selector";
        else if (head[1] !== undefined) {
          if (!head[1].startsWith("m")) throw bad("multiplexer tag");
          const value = head[1].slice(1);
          if (value.endsWith("M")) throw unsupported("extended multiplexing");
          const selected = unsigned(value, U64_MAX);
          if (selected === null) throw bad("multiplexer value");
          mux = selected;
        }
        const layout = definition[1].trimStart();
        const space = layout.search(WHITESPACE);
        if (space < 0) throw bad("signal layout");
        const position = splitOnce(layout.slice(0, space), "@");
        const bits = position && splitOnce(position[0], "|");
        if (!position || !bits) throw bad("signal layout");
        const start = unsigned(bits[0], 0xffffn);
        if (start === null) throw bad("start bit");
        const length = unsigned(bits[1], 0xffn);
        if (length === null) throw bad("signal length");
        if (length === 0n || length > 64n || start >= 512n)
          throw bad("signal position outside 1..=64 bits or 64 bytes");
        const order = position[1];
        if (!["1+", "1-", "0+", "0-"].includes(order))
          throw bad("byte order or sign");
        const after = layout.slice(space).trimStart();
        const scale = after.startsWith("(")
          ? splitOnce(after.slice(1), ")")
          : null;
        const pair = scale && splitOnce(scale[0], ",");
        if (!scale || !pair) throw bad("factor and offset");
        const factor = finiteOrNot(pair[0]);
        if (factor === null) throw bad("factor");
        const offset = finiteOrNot(pair[1]);
        if (offset === null) throw bad("offset");
        if (!Number.isFinite(factor) || !Number.isFinite(offset))
          throw bad("factor or offset is not finite");
        const unit = takeQuoted(scale[1]);
        if (!unit) throw bad("unit");
        (table.get(current) as Message).signals.push({
          name,
          start: Number(start),
          length: Number(length),
          littleEndian: order[0] === "1",
          kind: order[1] === "-" ? "signed" : "unsigned",
          factor,
          offset,
          unit: unit[0],
          mux,
          labels: [],
        });
      } else if (keyword === "SIG_VALTYPE_") {
        const fields = rest.split(/[\s:;]/).filter((part) => part !== "");
        const raw = unsigned(fields[0], 0xffff_ffffn);
        if (raw === null) throw bad("value type message");
        if (fields[1] === undefined) throw bad("value type signal");
        const kind = unsigned(fields[2], 0xffn);
        if (kind === null) throw bad("value type");
        floats.push([number, key(raw), fields[1], kind]);
      } else if (keyword === "VAL_") {
        // `VAL_ <message> <signal> <value> "<name>" ... ;`. Tables for
        // environment variables have no message identifier. A record that
        // does not end on this line, inside a name or between fields, is
        // left out whole rather than read in part.
        const message = field(rest);
        if (!message) continue;
        const raw = unsigned(message[0], 0xffff_ffffn);
        const named = field(message[1]);
        if (raw === null || !named) continue;
        if (inString || !rest.trimEnd().endsWith(";")) continue;
        const labels: [bigint, string][] = [];
        let pairs = named[1];
        for (;;) {
          pairs = pairs.trimStart();
          if (pairs === "" || pairs.startsWith(";")) break;
          const end = pairs.search(WHITESPACE);
          if (end < 0) throw bad("value table entry");
          const digits = pairs.slice(0, end);
          if (!/^[+-]?[0-9]+$/.test(digits)) throw bad("value table value");
          const value = BigInt(digits);
          if (value < I64_MIN || value > U64_MAX)
            throw bad("value table value");
          const label = takeQuoted(pairs.slice(end));
          if (!label) throw bad("value table name");
          labels.push([value, label[0]]);
          pairs = label[1];
        }
        tables.push([key(raw), named[0], labels]);
      } else if (keyword === "SG_MUL_VAL_") {
        throw unsupported("extended multiplexing");
      } else if (keyword !== "" && keyword !== "CM_") {
        // Any other record ends the signal list of the current message.
        current = null;
      }
    }
    for (const [message, name, labels] of tables) {
      const signal = table.get(message)?.signals.find((s) => s.name === name);
      if (signal) signal.labels = labels;
    }
    for (const [line, message, name, kind] of floats) {
      const signal = table.get(message)?.signals.find((s) => s.name === name);
      if (!signal || kind === 0n) continue;
      if (
        (kind === 1n && signal.length === 32) ||
        (kind === 2n && signal.length === 64)
      )
        signal.kind = "float";
      else
        throw new DbcError(
          "line",
          line,
          "float value type does not match the signal length",
        );
    }
    for (const message of table.values())
      if (message.signals.filter((s) => s.mux === "selector").length > 1)
        throw new DbcError(
          "unsupported",
          0,
          "several multiplexer selectors in one message",
        );
    return new DbcDatabase(table);
  }

  /**
   * Decode the signals of the message with this identifier, in file order.
   * Returns null when the database has no such message.
   */
  decode(
    id: number,
    extended: boolean,
    data: Uint8Array,
  ): DecodedMessage | null {
    const message = this.table.get(`${id}:${extended}`);
    if (!message) return null;
    const selectorSignal = message.signals.find((s) => s.mux === "selector");
    const selector = selectorSignal ? rawBits(data, selectorSignal) : null;
    const decoded: DecodedMessage = {
      name: message.name,
      signals: [],
      truncated: 0,
    };
    for (const signal of message.signals) {
      // Without a readable selector the active branch is unknown.
      if (typeof signal.mux === "bigint" && selector !== signal.mux) continue;
      const raw = rawBits(data, signal);
      if (raw === null) {
        decoded.truncated++;
        continue;
      }
      const [value, number] = physical(raw, signal);
      const integer = rawInteger(raw, signal);
      decoded.signals.push({
        name: signal.name,
        value,
        number,
        unit: signal.unit,
        label:
          integer === null
            ? null
            : (signal.labels.find(([labelled]) => labelled === integer)?.[1] ??
              null),
      });
    }
    return decoded;
  }
}

export function hexBytes(hex: string): Uint8Array {
  const bytes = new Uint8Array(hex.length >> 1);
  for (let i = 0; i < bytes.length; i++)
    bytes[i] = Number.parseInt(hex.slice(2 * i, 2 * i + 2), 16);
  return bytes;
}
/**
 * One line of text for a decoded message, in the layout the CLI prints:
 * `Engine: Speed=100 km/h, Gear=3 (Drive)`. Names and units come from a user
 * file, so control characters are replaced.
 */
export function signalText(message: DecodedMessage): string {
  const printable = (text: string) =>
    text.replace(/[\u0000-\u001f\u007f-\u009f]/g, "?");
  return `${printable(message.name)}: ${message.signals
    .map(
      (signal) =>
        `${printable(signal.name)}=${signal.value}` +
        (signal.label === null ? "" : ` (${printable(signal.label)})`) +
        (signal.unit ? ` ${printable(signal.unit)}` : ""),
    )
    .join(", ")}`.trimEnd();
}
