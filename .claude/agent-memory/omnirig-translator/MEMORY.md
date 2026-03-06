# OmniRig Translator Memory

## Key File Locations
- Schema: `/schema/transceiver.schema`
- Example rig file: `/rigs/IC-7300.rig` (ICOM CI-V binary protocol)
- Translated: `/rigs/FT-891.rig` (Yaesu ASCII CAT protocol)
- OmniRig source files: `/rig_files/*.ini`
- Parser: `/holyrig/src/runtime/parser.rs`
- Interpreter: `/holyrig/src/runtime/interpreter.rs`
- Data formats: `/holyrig/src/data_format.rs`
- Semantic analyzer: `/holyrig/src/runtime/semantic_analyzer.rs`
- Validator binary: `cargo run --bin parser -p holyrig -- --rig <file> --schema <schema>`

## Critical: ASCII vs Binary Protocol Handling
- `write()` accepts both `Value::Bytes` (from `"..."`) and `Value::String` (from `s"..."`)
- **For non-interpolated ASCII commands**: Use `s"..."` string syntax directly
  - Example: `write(s"AI0;")`, `write(s"RC;")`, `write(s"ST1;")`
  - This is cleaner and avoids hex encoding entirely
- **For interpolated templates** (containing `{var:format:len}`): Must use `"..."` byte syntax with hex-encoded literals
  - `HexString` has priority 3 > `Id` priority 2, so tokens like `FA`, `AB`, `FB`, `CF` match as hex bytes, NOT ASCII
  - Express ASCII characters as hex codes: `"46.41.{freq:text:9}.3B"` for `FA{freq};\`
  - Use `.` as visual separator between hex bytes
- `read()` templates always use `"..."` byte format with hex-encoded literals (interpolation required for parsing)
- ICOM protocols use raw binary bytes so hex strings work naturally for both write and read

## Supported Built-in Functions
- `write(bytes)` or `write(string)` - send data (accepts both `"..."` bytes and `s"..."` strings)
- `read(template)` - receive and parse data (length auto-calculated from template, always uses `"..."` byte format)
- `set_var(s"name", value)` - set status variable
- `error(s"message")` - report error (semantic analyzer accepts it, interpreter will bail)
- No `read()` needed for fire-and-forget commands (Yaesu CAT with ReplyLength=0)

## Data Formats
Available: `bcd_bs`, `bcd_bu`, `bcd_ls`, `bcd_lu`, `int_bs`, `int_bu`, `int_ls`, `int_lu`, `text`
- `text` handles digits 0-9 and leading `-` sign, but NOT `+` sign
- Variable syntax: `{name:format:length}` or `{name:length}` (no format = raw bytes)
- Wildcard: `{_:length}` skips bytes

## OmniRig vfText Format Mapping
- OmniRig `vfText` maps to holyrig `text` format
- `Value=start|len|vfText|mult|add` means: `decoded = raw * mult + add`
- Reverse for encoding: `raw = (value - add) / mult`

## Enum/Cast Rules
- `integer as Mode` works (looks up enum variant by value)
- `Mode::LSB as Mode` does NOT work (enum-to-enum cast is invalid)
- Empty function bodies cause validation error; use `error(s"...")` instead

## Yaesu FT-891 Specific Notes
- Mode digit 9 (DATA-FM) maps to DIGIU per OmniRig definition
- Modes A-E are hex digits that `text` format cannot decode
- RIT offset in IF response includes +/- sign; `+` breaks text decoder
- CW pitch: KP value 00-99, pitch_hz = value * 10 + 300
- Clarifier (RIT) uses CF command, not RT command
