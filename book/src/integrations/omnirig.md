# OmniRig

HolyRig implements an OmniRig compatible COM server (`IOmniRigX` / `IRigX`).
Applications that already work with OmniRig can connect to HolyRig without modification.

## Supported

The core rig control surface is fully implemented:

- **Frequencies** — read and write FreqA, FreqB, Freq (active VFO)
- **Mode** — CW (U/L), SSB (U/L), DIGI (U/L), AM, FM
- **VFO** — select A/B, set RX/TX VFO independently (VfoAA, VfoAB, etc.), VfoEqual, VfoSwap
- **Split** — on/off
- **RIT/XIT** — on/off, offset read/write
- **TX** — transmit/receive state
- **CW Pitch** — read and write
- **Status** — online/not responding, rig type, readable/writeable params bitmask
- **Composite methods** — `ClearRit`, `SetSimplexMode`, `SetSplitMode`, `FrequencyOfTone`, `GetRxFrequency`, `GetTxFrequency`
- **Two rigs** — Rig1 and Rig2 are both available
- **IDispatch** — late binding works (VBA `CreateObject`, Python `win32com`, etc.)

## Not Supported

- **Events** — `IOmniRigXEvents` is not implemented. Clients must poll for status changes.
- **SendCustomCommand** — the COM method exists and accepts the call, but it is a no-op. There is no raw serial passthrough.
- **PortBits** — the `PortBits` property returns a dummy object. RTS/DTR/CTS/DSR control has no effect.
- **Type library** — no `.tlb` is generated. Early binding (e.g. VBA References dialog) won't find a type library. Late binding works fine.
- **CustomReply event** — since events are not implemented, there is no way to receive replies from custom commands.

## Quick Reference

### Write Operations

| OmniRig | HolyRig | Notes |
|---|---|---|
| `put_Freq(val)` | `set_freq(val, Current)` | |
| `put_FreqA(val)` | `set_freq(val, A)` | |
| `put_FreqB(val)` | `set_freq(val, B)` | |
| `put_RitOffset(val)` | `rit_offset(val)` | |
| `put_Pitch(val)` | `cw_pitch(val)` | |
| `put_Vfo(VfoAA)` | `set_vfo(A, A)` | See [translation details](../implementation/omnirig_translation.md#vfo-operations) |
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
| `ClearRit()` | `rit_offset(0)` | Sets offset to zero |
| `SetSimplexMode(f)` | Multiple calls | Sets both VFOs to f, disables split/RIT/XIT |
| `SetSplitMode(rx, tx)` | Multiple calls | Sets FreqA=rx, FreqB=tx, enables split |
| `FrequencyOfTone(t)` | — | Computed from mode, pitch, freq |
| `GetRxFrequency()` | — | Computed from VFO, freq, RIT |
| `GetTxFrequency()` | — | Computed from VFO, freq, split, XIT |
| `SendCustomCommand(...)` | — | No-op stub |

### Read Operations (Status)

| OmniRig | HolyRig status field |
|---|---|
| `get_Freq()` | `freq_a` or `freq_b` (depends on active VFO) |
| `get_FreqA()` | `freq_a` |
| `get_FreqB()` | `freq_b` |
| `get_RitOffset()` | `rit_offset` |
| `get_Pitch()` | `cw_pitch` |
| `get_Vfo()` | `vfo` |
| `get_Split()` | `split` |
| `get_Rit()` | `rit` |
| `get_Xit()` | `xit` |
| `get_Tx()` | `transmit` |
| `get_Mode()` | `mode` |
