# DBC signal decoding

`canharbor monitor --format text --dbc vehicle.dbc` appends decoded signal
values to each frame the file describes:

```text
     1.536481 RX      123 [ 3] C8 00 3C  Engine: Speed=100 km/h, Temp=20 degC
     1.544260 RX      124 [ 3] C8 00 3C
```

Decoding happens in the CLI process and is presentation only. It does not
change the subscription, the JSON output, recordings or anything that is
transmitted. Frames without a matching message are printed unchanged. Standard
and extended identifiers with the same number are different messages.

## In the workbench

The frame monitor has an optional **DBC file** control. With a file loaded, the
recent-frames table gets a **Signals** column in the same layout as the CLI
output. Frames already in the table are decoded as well, and a signal beyond
the received payload is counted at the end of the cell. The by-identifier view
shows the signals of the latest frame of each group, and the offline capture
viewer shows them for the events of the page on display. The offline viewer
has the same file control; one database applies to all three tables.

The file is read and decoded in the page. It is not sent to the service, is not
stored, and has to be chosen again after a reload. A file that is rejected
reports its line and leaves the previous database in place.

The page uses its own implementation of the rules below
(`web/src/dbc.ts`), kept in step with the Rust crate by running the same
reference data and edge cases against both.

## Supported subset

- `BO_` messages and `SG_` signals in either byte order, signed or unsigned,
  1 to 64 bits, within the first 64 bytes.
- 32-bit and 64-bit IEEE floats declared with `SIG_VALTYPE_`.
- Linear scaling (`factor`, `offset`) and units. Integers with factor 1 and
  offset 0 are printed exactly; scaled values are printed as 64-bit floats.
- Value tables (`VAL_`): when the raw value of an integer signal has a name,
  it is printed after the value, for example `Gear=3 (Drive)`. Names apply to
  the raw value, before scaling.
- Simple multiplexing: one `M` selector per message with `m<N>` signals. A
  multiplexed signal is shown only when the selector equals its value.

Attributes, comments, signal groups, minimum/maximum ranges, and value tables
for environment variables are read past and not used. A value table must end with its
semicolon on the line it starts on; one that continues on a following line is
left out whole. Files that use extended multiplexing
(`SG_MUL_VAL_` or `m<N>M` signals) are rejected: decoding them with the simple
rules would print wrong values. A malformed signal definition is also an error,
reported with its line number. Files larger than 16 MiB are refused.

Files that are valid UTF-8 are read as UTF-8. Any other file is read as
Windows-1252, the usual encoding of DBC files, so units such as `°C` keep their
characters.

A signal whose bits lie beyond the received payload is not printed; the line
ends with the number of such signals instead of showing a value of zero. Remote
frames carry no data and are never decoded. Names and units are printed with
control characters replaced.

## Verification

The decoder is compared against cantools 45.1.2 in
`crates/canharbor-dbc/tests/reference.rs`: 421 generated messages covering both
byte orders, signed and unsigned integers from 1 to 64 bits (61 of the 64 lengths occur),
floats, scaling, standard and extended identifiers and classic and CAN FD
lengths, with six random payloads each. The reference files were generated for
this project and contain no third-party database. No vendor DBC file or
physical bus was used.

`web/tests/dbc.spec.ts` runs the same reference files, and the edge cases of
the Rust tests, against the page's decoder. One difference is intended: the
page prints scaled values with JavaScript's number formatting, which switches
to exponent notation for very large and very small magnitudes where the CLI
does not.
