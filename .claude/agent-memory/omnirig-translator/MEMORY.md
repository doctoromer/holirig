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
- The `.rig` string literals (in `"..."`) are parsed as hex when content has NO `{}` interpolation
- Strings with `{}` go through `StringToken` lexer where `Id` tokens become ASCII bytes
- **Problem**: `HexString` has priority 3 > `Id` priority 2. Tokens like `FA`, `AB`, `FB`, `CF` match as hex bytes, NOT ASCII!
- **Solution for ASCII protocols (Yaesu, Kenwood)**: Express ALL characters as hex ASCII codes
  - Example: `AI0;` becomes `"41.49.30.3B"` (A=41, I=49, 0=30, ;=3B)
  - Use `.` as visual separator (produces empty bytes)
- ICOM protocols use raw binary bytes so hex strings work naturally

## Supported Built-in Functions
- `write(bytes)` - send data
- `read(template)` - receive and parse data (length auto-calculated from template)
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
