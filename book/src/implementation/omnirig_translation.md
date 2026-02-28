# OmniRig to HolyRig Translation

This document maps every OmniRig COM operation to its HolyRig Transceiver schema
equivalent. It serves as both a reference spec and an implementation guide for building
the OmniRig COM provider backed by HolyRig.

Direction: OmniRig -> HolyRig only.

## Key Architectural Differences

**Type system.** OmniRig uses `RigParamX` i32 bit flags for on/off states and enum
values (e.g. `SplitOn = 32768`, `SplitOff = 65536`). HolyRig uses native `bool` and
`enum` types (e.g. `set_split(true)`).

**API style.** OmniRig is property-based: get/put pairs on a COM object (`get_Freq`,
`put_Freq`). HolyRig is function-based: schema commands (`set_freq(freq, target)`)
with a separate polled `status` block for read-back.

**VFO model.** OmniRig encodes RX/TX VFO combinations as single enum values (`VfoAA`,
`VfoAB`, `VfoBA`, `VfoBB`). HolyRig's `set_vfo(rx, tx)` takes two separate `Vfo`
arguments.

## Quick Reference

### Write Operations

| OmniRig | HolyRig | Notes |
|---|---|---|
| `put_Freq(val)` | `set_freq(val, Current)` | |
| `put_FreqA(val)` | `set_freq(val, A)` | |
| `put_FreqB(val)` | `set_freq(val, B)` | |
| `put_RitOffset(val)` | — | Extension needed |
| `put_Pitch(val)` | `cw_pitch(val)` | |
| `put_Vfo(VfoAA)` | `set_vfo(A, A)` | See VFO table below |
| `put_Vfo(VfoAB)` | `set_vfo(A, B)` | |
| `put_Vfo(VfoBA)` | `set_vfo(B, A)` | |
| `put_Vfo(VfoBB)` | `set_vfo(B, B)` | |
| `put_Vfo(VfoA)` | `set_vfo(A, Current)` | TX unchanged |
| `put_Vfo(VfoB)` | `set_vfo(B, Current)` | TX unchanged |
| `put_Vfo(VfoEqual)` | `vfo_equal()` | Copy A to B |
| `put_Vfo(VfoSwap)` | `vfo_swap()` | Swap A and B |
| `put_Split(SplitOn)` | `set_split(true)` | |
| `put_Split(SplitOff)` | `set_split(false)` | |
| `put_Rit(RitOn)` | `set_rit(true)` | |
| `put_Rit(RitOff)` | `set_rit(false)` | |
| `put_Xit(XitOn)` | `set_xit(true)` | |
| `put_Xit(XitOff)` | `set_xit(false)` | |
| `put_Tx(Tx)` | `transmit(true)` | |
| `put_Tx(Rx)` | `transmit(false)` | |
| `put_Mode(CwU)` | `set_mode(CWU)` | |
| `put_Mode(CwL)` | `set_mode(CWL)` | |
| `put_Mode(SsbU)` | `set_mode(USB)` | Name differs |
| `put_Mode(SsbL)` | `set_mode(LSB)` | Name differs |
| `put_Mode(DigU)` | `set_mode(DIGIU)` | Name differs |
| `put_Mode(DigL)` | `set_mode(DIGIL)` | Name differs |
| `put_Mode(Am)` | `set_mode(AM)` | |
| `put_Mode(Fm)` | `set_mode(FM)` | |

### Methods

| OmniRig | HolyRig | Notes |
|---|---|---|
| `ClearRit()` | `clear_rit()` | Direct match |
| `SetSimplexMode(f)` | Multiple calls | See composite methods |
| `SetSplitMode(rx, tx)` | Multiple calls | See composite methods |
| `FrequencyOfTone(t)` | — | Computable |
| `GetRxFrequency()` | — | Computable from status |
| `GetTxFrequency()` | — | Computable from status |
| `SendCustomCommand(...)` | — | Extension needed |

### Read Operations (Status)

| OmniRig | HolyRig status field | Notes |
|---|---|---|
| `get_Freq()` | `freq_a` or `freq_b` | Depends on active VFO |
| `get_FreqA()` | `freq_a` | |
| `get_FreqB()` | `freq_b` | |
| `get_RitOffset()` | — | Extension needed |
| `get_Pitch()` | `cw_pitch` | |
| `get_Vfo()` | `vfo` | Simpler; no RX/TX encoding |
| `get_Split()` | — | Extension needed (commented out) |
| `get_Rit()` | `rit` | |
| `get_Xit()` | `xit` | |
| `get_Tx()` | `transmit` | |
| `get_Mode()` | `mode` | |

### No HolyRig Equivalent

| OmniRig | Category |
|---|---|
| `RigType` | Device management |
| `Status` | Device management |
| `StatusStr` | Device management |
| `ReadableParams` / `WriteableParams` | Device management |
| `IsParamReadable` / `IsParamWriteable` | Device management |
| `PortBits` (RTS/DTR/CTS/DSR) | Serial port control |
| `SendCustomCommand` | Raw serial passthrough |

---

## Detailed Translations

### Frequency Operations

#### `Freq` (property)

OmniRig's "current frequency" — the frequency of whichever VFO is active.

**Write:** `put_Freq(val: i32)` where val is frequency in Hz.

```
HolyRig: set_freq(val, Vfo::Current)
```

**Read:** `get_Freq() -> i32`

HolyRig has no "current frequency" status field. Derive it from the active VFO:
- If `status.vfo == A` -> return `status.freq_a`
- If `status.vfo == B` -> return `status.freq_b`
- Otherwise -> return `status.freq_a` (default)

#### `FreqA` (property)

**Write:** `put_FreqA(val: i32)`

```
HolyRig: set_freq(val, Vfo::A)
```

**Read:** `get_FreqA() -> i32`

```
HolyRig: status.freq_a
```

#### `FreqB` (property)

**Write:** `put_FreqB(val: i32)`

```
HolyRig: set_freq(val, Vfo::B)
```

**Read:** `get_FreqB() -> i32`

```
HolyRig: status.freq_b
```

#### `RitOffset` (property)

**Write:** `put_RitOffset(val: i32)` where val is offset in Hz.

```
HolyRig: No equivalent. Extension needed — see Proposed Schema Extensions.
```

**Read:** `get_RitOffset() -> i32`

```
HolyRig: No status field. Extension needed.
```

The `rit_offset` function is commented out in `transceiver.schema`. Uncommenting it
would cover this operation.

#### `Pitch` (property)

CW sidetone pitch in Hz.

**Write:** `put_Pitch(val: i32)`

```
HolyRig: cw_pitch(val)
```

**Read:** `get_Pitch() -> i32`

```
HolyRig: status.cw_pitch
```

---

### VFO Operations

#### `Vfo` (property)

OmniRig overloads this single property for multiple VFO operations. The i32 value
determines the action.

**Write:** `put_Vfo(val: i32)` — translate based on the `RigParamX` value:

| RigParamX | Value | HolyRig call | Semantics |
|---|---|---|---|
| `VfoAA` | 128 | `set_vfo(Vfo::A, Vfo::A)` | RX on A, TX on A |
| `VfoAB` | 256 | `set_vfo(Vfo::A, Vfo::B)` | RX on A, TX on B |
| `VfoBA` | 512 | `set_vfo(Vfo::B, Vfo::A)` | RX on B, TX on A |
| `VfoBB` | 1024 | `set_vfo(Vfo::B, Vfo::B)` | RX on B, TX on B |
| `VfoA` | 2048 | `set_vfo(Vfo::A, Vfo::Current)` | Select A for RX, TX unchanged |
| `VfoB` | 4096 | `set_vfo(Vfo::B, Vfo::Current)` | Select B for RX, TX unchanged |
| `VfoEqual` | 8192 | `vfo_equal()` | Copy VFO A to VFO B |
| `VfoSwap` | 16384 | `vfo_swap()` | Swap VFO A and B |

**Read:** `get_Vfo() -> i32`

Returns one of the `RigParamX` VFO values above.

HolyRig's `status.vfo` is simpler — it returns `Vfo::A`, `Vfo::B`, `Vfo::Current`,
or `Vfo::Unknown`. It does **not** encode the RX/TX VFO combination.

To reconstruct the OmniRig value from HolyRig status, you would need to know both
the RX and TX VFO separately. If only `status.vfo` is available, the best
approximation is:
- `Vfo::A` -> `VfoA` (2048)
- `Vfo::B` -> `VfoB` (4096)
- Otherwise -> `Unknown` (1)

---

### Mode & State

#### `Mode` (property)

**Write:** `put_Mode(val: i32)` — translate `RigParamX` to HolyRig `Mode`:

| RigParamX | Value | HolyRig Mode |
|---|---|---|
| `CwU` | 8388608 | `Mode::CWU` |
| `CwL` | 16777216 | `Mode::CWL` |
| `SsbU` | 33554432 | `Mode::USB` |
| `SsbL` | 67108864 | `Mode::LSB` |
| `DigU` | 134217728 | `Mode::DIGIU` |
| `DigL` | 268435456 | `Mode::DIGIL` |
| `Am` | 536870912 | `Mode::AM` |
| `Fm` | 1073741824 | `Mode::FM` |

Note the naming differences: OmniRig uses `SsbU`/`SsbL` where HolyRig uses
`USB`/`LSB`, and `DigU`/`DigL` vs `DIGIU`/`DIGIL`.

**Read:** `get_Mode() -> i32`

```
HolyRig: status.mode (reverse the table above)
```

#### `Split` (property)

**Write:** `put_Split(val: i32)`

| RigParamX | Value | HolyRig call |
|---|---|---|
| `SplitOn` | 32768 | `set_split(true)` |
| `SplitOff` | 65536 | `set_split(false)` |

**Read:** `get_Split() -> i32`

HolyRig's status block has `split` commented out. Until the schema is extended,
this cannot be read back. See Proposed Schema Extensions.

#### `Rit` (property)

**Write:** `put_Rit(val: i32)`

| RigParamX | Value | HolyRig call |
|---|---|---|
| `RitOn` | 131072 | `set_rit(true)` |
| `RitOff` | 262144 | `set_rit(false)` |

**Read:** `get_Rit() -> i32`

```
HolyRig: status.rit -> RitOn (131072) if true, RitOff (262144) if false
```

#### `Xit` (property)

**Write:** `put_Xit(val: i32)`

| RigParamX | Value | HolyRig call |
|---|---|---|
| `XitOn` | 524288 | `set_xit(true)` |
| `XitOff` | 1048576 | `set_xit(false)` |

**Read:** `get_Xit() -> i32`

```
HolyRig: status.xit -> XitOn (524288) if true, XitOff (1048576) if false
```

#### `Tx` (property)

**Write:** `put_Tx(val: i32)`

| RigParamX | Value | HolyRig call |
|---|---|---|
| `Tx` | 4194304 | `transmit(true)` |
| `Rx` | 2097152 | `transmit(false)` |

**Read:** `get_Tx() -> i32`

```
HolyRig: status.transmit -> Tx (4194304) if true, Rx (2097152) if false
```

---

### Composite Methods

#### `ClearRit()`

Resets the RIT offset to zero.

```
HolyRig: clear_rit()
```

Direct match.

#### `SetSimplexMode(freq: i32)`

Sets simplex operation: same frequency on both VFOs, split/RIT/XIT disabled.

OmniRig implementation (`omnirig/src/rig.rs:310-318`):
```rust
self.inner.set_freq(freq);
self.inner.set_freq_a(freq);
self.inner.set_freq_b(freq);
self.inner.set_split(RigParamX::SplitOff);
self.inner.set_rit(RigParamX::RitOff);
self.inner.set_xit(RigParamX::XitOff);
```

HolyRig equivalent sequence:
```
set_freq(freq, Vfo::Current)
set_freq(freq, Vfo::A)
set_freq(freq, Vfo::B)
set_split(false)
set_rit(false)
set_xit(false)
```

#### `SetSplitMode(rx_freq: i32, tx_freq: i32)`

Sets split operation: VFO A for RX, VFO B for TX, RIT/XIT disabled.

OmniRig implementation (`omnirig/src/rig.rs:322-332`):
```rust
self.inner.set_freq_a(rx_freq);
self.inner.set_freq_b(tx_freq);
self.inner.set_split(RigParamX::SplitOn);
self.inner.set_rit(RigParamX::RitOff);
self.inner.set_xit(RigParamX::XitOff);
```

HolyRig equivalent sequence:
```
set_freq(rx_freq, Vfo::A)
set_freq(tx_freq, Vfo::B)
set_split(true)
set_rit(false)
set_xit(false)
```

#### `FrequencyOfTone(tone: i32) -> i32`

Computes the dial frequency needed to produce a given audio tone. No direct HolyRig
equivalent — this is a pure computation.

Algorithm (`omnirig/src/rig.rs:336-348`):
```
result = tone
if mode is CwU or CwL:
    result -= pitch
if mode is CwL or SsbL:
    result = -result
result += freq
return result
```

Can be computed in the translation layer using HolyRig status fields:
`status.mode`, `status.cw_pitch`, and `status.freq_a`/`status.freq_b`.

#### `GetRxFrequency() -> i32`

Returns the actual receive frequency accounting for VFO selection and RIT offset.
No direct HolyRig equivalent — computable from status fields.

Algorithm (`omnirig/src/rig.rs:366-386`):
```
base = match vfo:
    VfoA, VfoAA, VfoAB -> freq_a
    VfoB, VfoBA, VfoBB -> freq_b
    _ -> if not transmitting or split off: freq, else 0

if rit is on:
    base += rit_offset

return base
```

With HolyRig status fields, the simplified version (without RIT offset, which
requires the schema extension):
```
base = match status.vfo:
    A -> status.freq_a
    B -> status.freq_b
    _ -> status.freq_a  (default)
```

#### `GetTxFrequency() -> i32`

Returns the actual transmit frequency accounting for VFO selection, split mode, and
XIT offset. No direct HolyRig equivalent — computable from status fields.

Algorithm (`omnirig/src/rig.rs:389-413`):
```
base = match vfo:
    VfoAA, VfoBA -> freq_a
    VfoAB, VfoBB -> freq_b
    VfoA + split off -> freq_a
    VfoA + split on  -> freq_b
    VfoB + split off -> freq_b
    VfoB + split on  -> freq_a
    _ -> if transmitting: freq, else 0

if xit is on:
    base += rit_offset  (note: uses rit_offset, not a separate xit_offset)

return base
```

This algorithm depends on the VFO RX/TX combination which HolyRig's status does not
fully capture. Requires schema extension for full fidelity.

#### `SendCustomCommand(command: bytes, reply_length: i32, reply_end: bytes)`

Sends raw bytes to the rig's serial port and optionally waits for a reply.
No HolyRig equivalent. Would require a raw serial passthrough extension.

---

### Metadata

These OmniRig operations relate to device identity and connection management, not
transceiver control. They belong to a layer above the Transceiver schema.

| OmniRig | Purpose | HolyRig |
|---|---|---|
| `RigType` | Rig model name (string) | No equivalent |
| `Status` | Connection status (`RigStatusX` enum) | No equivalent |
| `StatusStr` | Human-readable status | No equivalent |
| `ReadableParams` | Bitmask of readable parameters | No equivalent |
| `WriteableParams` | Bitmask of writable parameters | No equivalent |
| `IsParamReadable(p)` | Check if parameter is readable | No equivalent |
| `IsParamWriteable(p)` | Check if parameter is writable | No equivalent |

`RigStatusX` values for reference:

| Value | Name | Meaning |
|---|---|---|
| 0 | NotConfigured | Rig not configured |
| 1 | Disabled | Rig disabled |
| 2 | PortBusy | Serial port busy |
| 3 | NotResponding | Rig not responding |
| 4 | Online | Rig online and ready |

---

### PortBits

Serial port handshake line control. Entirely separate from rig commands — operates
at the physical serial port level. No HolyRig equivalent.

| OmniRig | Direction | Purpose |
|---|---|---|
| `Lock() -> bool` | — | Acquire exclusive port access |
| `Unlock()` | — | Release port lock |
| `Rts` (get/set) | Output | Request To Send |
| `Dtr` (get/set) | Output | Data Terminal Ready |
| `Cts` (get) | Input | Clear To Send |
| `Dsr` (get) | Input | Data Set Ready |

---

## Proposed Schema Extensions

### Easy Wins

Already scaffolded in `transceiver.schema` (commented out):

**`rit_offset(int offset)`** — Covers `put_RitOffset` / `get_RitOffset`. Uncomment
the function in the schema and add `int rit_offset` to the status block.

**`bool split` in status block** — Covers `get_Split`. Uncomment the field in the
status block.

### Computable in Translation Layer

These don't need schema changes — implement the algorithms in the OmniRig provider:

**`GetRxFrequency`** — Compute from `status.vfo` + `status.freq_a` / `status.freq_b`
+ `status.rit` + rit_offset (if extended).

**`GetTxFrequency`** — Compute from VFO selection + split + XIT + frequencies.
Requires knowing the TX VFO, which HolyRig's status doesn't currently expose.

**`FrequencyOfTone`** — Compute from `status.mode` + `status.cw_pitch` + frequency.

### Requires New Capability

**`SendCustomCommand`** — Raw byte passthrough to the rig's serial port. Would need
a new HolyRig command for direct serial access, bypassing the schema command layer.

**`PortBits`** — Serial line control (RTS/DTR/CTS/DSR). Independent of rig commands.
Would need a separate interface for serial port signal management.

### Out of Scope

These belong to device management, not the Transceiver schema:

- `RigType`, `Status`, `StatusStr` — rig identity and connection state
- `ReadableParams`, `WriteableParams` — capability discovery
- `IsParamReadable`, `IsParamWriteable` — capability queries

These would be handled at the device/connection management layer, which is separate
from the per-rig command translation.
