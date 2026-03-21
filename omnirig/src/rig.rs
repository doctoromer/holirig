#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::sync::RwLock;
use windows::core::{implement, BSTR};
use windows::Win32::System::Com::{IDispatch, IDispatch_Impl, IDispatch_Vtbl};
use windows::Win32::System::Variant::{VariantGetElementCount, VariantToBuffer, VARIANT};
use windows_core::{interface, Interface, HRESULT};

use crate::enums::RigParamX;
use crate::port_bits::{IPortBits, PortBits};
use crate::provider::RigControl;
use auto_dispatch::auto_dispatch;

#[interface("D30A7E51-5862-45B7-BFFA-6415917DA0CF")]
pub unsafe trait IRigX: IDispatch {
    pub fn get_RigType(&self, value: *mut BSTR) -> HRESULT;
    pub fn get_ReadableParams(&self, value: *mut i32) -> HRESULT;
    pub fn get_WriteableParams(&self, value: *mut i32) -> HRESULT;
    pub fn IsParamReadable(&self, Param: i32, value: *mut bool) -> HRESULT;
    pub fn IsParamWriteable(&self, Param: i32, value: *mut bool) -> HRESULT;
    pub fn get_Status(&self, value: *mut i32) -> HRESULT;
    pub fn get_StatusStr(&self, value: *mut BSTR) -> HRESULT;
    pub fn get_Freq(&self, value: *mut i32) -> HRESULT;
    pub fn put_Freq(&self, value: i32) -> HRESULT;
    pub fn get_FreqA(&self, value: *mut i32) -> HRESULT;
    pub fn put_FreqA(&self, value: i32) -> HRESULT;
    pub fn get_FreqB(&self, value: *mut i32) -> HRESULT;
    pub fn put_FreqB(&self, value: i32) -> HRESULT;
    pub fn get_RitOffset(&self, value: *mut i32) -> HRESULT;
    pub fn put_RitOffset(&self, value: i32) -> HRESULT;
    pub fn get_Pitch(&self, value: *mut i32) -> HRESULT;
    pub fn put_Pitch(&self, value: i32) -> HRESULT;
    pub fn get_Vfo(&self, value: *mut i32) -> HRESULT;
    pub fn put_Vfo(&self, value: i32) -> HRESULT;
    pub fn get_Split(&self, value: *mut i32) -> HRESULT;
    pub fn put_Split(&self, value: i32) -> HRESULT;
    pub fn get_Rit(&self, value: *mut i32) -> HRESULT;
    pub fn put_Rit(&self, value: i32) -> HRESULT;
    pub fn get_Xit(&self, value: *mut i32) -> HRESULT;
    pub fn put_Xit(&self, value: i32) -> HRESULT;
    pub fn get_Tx(&self, value: *mut i32) -> HRESULT;
    pub fn put_Tx(&self, value: i32) -> HRESULT;
    pub fn get_Mode(&self, value: *mut i32) -> HRESULT;
    pub fn put_Mode(&self, value: i32) -> HRESULT;
    pub fn ClearRit(&self) -> HRESULT;
    pub fn SetSimplexMode(&self, Freq: i32) -> HRESULT;
    pub fn SetSplitMode(&self, RxFreq: i32, TxFreq: i32) -> HRESULT;
    pub fn FrequencyOfTone(&self, Tone: i32, value: *mut i32) -> HRESULT;
    pub fn SendCustomCommand(
        &self,
        Command: VARIANT,
        ReplyLength: i32,
        ReplyEnd: VARIANT,
    ) -> HRESULT;
    pub fn GetRxFrequency(&self, value: *mut i32) -> HRESULT;
    pub fn GetTxFrequency(&self, value: *mut i32) -> HRESULT;
    pub fn get_PortBits(&self, value: *mut Option<IDispatch>) -> HRESULT;
}

#[implement(IRigX)]
pub struct RigX {
    inner: Box<dyn RigControl>,
    port_bits_com: RwLock<Option<IPortBits>>,
}

impl RigX {
    pub fn new(inner: Box<dyn RigControl>) -> Self {
        let port_bits_com = inner
            .port_bits()
            .map(|pb| -> IPortBits { PortBits::new(pb).into() });

        Self {
            inner,
            port_bits_com: RwLock::new(port_bits_com),
        }
    }
}

fn variant_to_bytes(variant: &VARIANT) -> Result<Vec<u8>, HRESULT> {
    unsafe {
        let count = VariantGetElementCount(variant) as usize;
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = vec![0u8; count];
        VariantToBuffer(variant, buffer.as_mut_ptr() as _, count as u32)
            .map_err(|e| HRESULT(e.code().0))?;
        Ok(buffer)
    }
}

#[auto_dispatch]
impl RigX {
    #[id(0x01)]
    #[getter]
    fn RigType(&self) -> Result<BSTR, HRESULT> {
        println!("RigX::RigType getter called");
        Ok(BSTR::from(self.inner.rig_type()))
    }

    #[id(0x02)]
    #[getter]
    fn ReadableParams(&self) -> Result<i32, HRESULT> {
        println!("RigX::ReadableParams getter called");
        Ok(self.inner.readable_params())
    }

    #[id(0x03)]
    #[getter]
    fn WriteableParams(&self) -> Result<i32, HRESULT> {
        println!("RigX::WriteableParams getter called");
        Ok(self.inner.writeable_params())
    }

    #[id(0x04)]
    fn IsParamReadable(&self, param: i32) -> Result<bool, HRESULT> {
        println!("RigX::IsParamReadable called with param: {}", param);
        Ok((self.inner.readable_params() & param) != 0)
    }

    #[id(0x05)]
    fn IsParamWriteable(&self, param: i32) -> Result<bool, HRESULT> {
        println!("RigX::IsParamWriteable called with param: {}", param);
        Ok((self.inner.writeable_params() & param) != 0)
    }

    #[id(0x07)]
    #[getter]
    fn StatusStr(&self) -> Result<BSTR, HRESULT> {
        println!("RigX::StatusStr getter called");
        Ok(BSTR::from(self.inner.status_str()))
    }

    #[id(0x08)]
    #[getter]
    fn Freq(&self) -> Result<i32, HRESULT> {
        println!("RigX::Freq getter called");
        Ok(self.inner.freq())
    }

    #[id(0x08)]
    #[setter]
    fn Freq(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Freq setter called with value: {}", value);
        self.inner.set_freq(value);
        Ok(())
    }

    #[id(0x09)]
    #[getter]
    fn FreqA(&self) -> Result<i32, HRESULT> {
        println!("RigX::FreqA getter called");
        Ok(self.inner.freq_a())
    }

    #[id(0x09)]
    #[setter]
    fn FreqA(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::FreqA setter called with value: {}", value);
        self.inner.set_freq_a(value);
        Ok(())
    }

    #[id(0x0A)]
    #[getter]
    fn FreqB(&self) -> Result<i32, HRESULT> {
        println!("RigX::FreqB getter called");
        Ok(self.inner.freq_b())
    }

    #[id(0x0A)]
    #[setter]
    fn FreqB(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::FreqB setter called with value: {}", value);
        self.inner.set_freq_b(value);
        Ok(())
    }

    #[id(0x0B)]
    #[getter]
    fn RitOffset(&self) -> Result<i32, HRESULT> {
        println!("RigX::RitOffset getter called");
        Ok(self.inner.rit_offset())
    }

    #[id(0x0B)]
    #[setter]
    fn RitOffset(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::RitOffset setter called with value: {}", value);
        self.inner.set_rit_offset(value);
        Ok(())
    }

    #[id(0x0C)]
    #[getter]
    fn Pitch(&self) -> Result<i32, HRESULT> {
        println!("RigX::Pitch getter called");
        Ok(self.inner.pitch())
    }

    #[id(0x0C)]
    #[setter]
    fn Pitch(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Pitch setter called with value: {}", value);
        self.inner.set_pitch(value);
        Ok(())
    }

    #[id(0x0D)]
    #[getter]
    fn Vfo(&self) -> Result<i32, HRESULT> {
        println!("RigX::Vfo getter called");
        Ok(self.inner.vfo().into())
    }

    #[id(0x0D)]
    #[setter]
    fn Vfo(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Vfo setter called with value: {}", value);
        self.inner.set_vfo(RigParamX::from(value));
        Ok(())
    }

    #[id(0x0E)]
    #[getter]
    fn Split(&self) -> Result<i32, HRESULT> {
        println!("RigX::Split getter called");
        Ok(self.inner.split().into())
    }

    #[id(0x0E)]
    #[setter]
    fn Split(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Split setter called with value: {}", value);
        self.inner.set_split(RigParamX::from(value));
        Ok(())
    }

    #[id(0x0F)]
    #[getter]
    fn Rit(&self) -> Result<i32, HRESULT> {
        println!("RigX::Rit getter called");
        Ok(self.inner.rit().into())
    }

    #[id(0x0F)]
    #[setter]
    fn Rit(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Rit setter called with value: {}", value);
        self.inner.set_rit(RigParamX::from(value));
        Ok(())
    }

    #[id(0x10)]
    #[getter]
    fn Xit(&self) -> Result<i32, HRESULT> {
        println!("RigX::Xit getter called");
        Ok(self.inner.xit().into())
    }

    #[id(0x10)]
    #[setter]
    fn Xit(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Xit setter called with value: {}", value);
        self.inner.set_xit(RigParamX::from(value));
        Ok(())
    }

    #[id(0x11)]
    #[getter]
    fn Tx(&self) -> Result<i32, HRESULT> {
        println!("RigX::Tx getter called");
        Ok(self.inner.tx().into())
    }

    #[id(0x11)]
    #[setter]
    fn Tx(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Tx setter called with value: {}", value);
        self.inner.set_tx(RigParamX::from(value));
        Ok(())
    }

    #[id(0x12)]
    #[getter]
    fn Mode(&self) -> Result<i32, HRESULT> {
        println!("RigX::Mode getter called");
        Ok(self.inner.mode().into())
    }

    #[id(0x12)]
    #[setter]
    fn Mode(&self, value: i32) -> Result<(), HRESULT> {
        println!("RigX::Mode setter called with value: {}", value);
        self.inner.set_mode(RigParamX::from(value));
        Ok(())
    }

    #[id(0x06)]
    #[getter]
    fn Status(&self) -> Result<i32, HRESULT> {
        println!("RigX::Status getter called");
        Ok(self.inner.status().into())
    }

    #[id(0x13)]
    fn ClearRit(&self) -> Result<(), HRESULT> {
        println!("RigX::ClearRit called");
        self.inner.set_rit_offset(0);
        Ok(())
    }

    #[id(0x14)]
    fn SetSimplexMode(&self, freq: i32) -> Result<(), HRESULT> {
        println!("RigX::SetSimplexMode called with freq: {}", freq);
        self.inner.set_freq(freq);
        self.inner.set_freq_a(freq);
        self.inner.set_freq_b(freq);
        self.inner.set_split(RigParamX::SplitOff);
        self.inner.set_rit(RigParamX::RitOff);
        self.inner.set_xit(RigParamX::XitOff);
        Ok(())
    }

    #[id(0x15)]
    fn SetSplitMode(&self, rx_freq: i32, tx_freq: i32) -> Result<(), HRESULT> {
        println!(
            "RigX::SetSplitMode called with rx_freq: {}, tx_freq: {}",
            rx_freq, tx_freq
        );
        self.inner.set_freq_a(rx_freq);
        self.inner.set_freq_b(tx_freq);
        self.inner.set_split(RigParamX::SplitOn);
        self.inner.set_rit(RigParamX::RitOff);
        self.inner.set_xit(RigParamX::XitOff);
        Ok(())
    }

    #[id(0x16)]
    fn FrequencyOfTone(&self, tone: i32) -> Result<i32, HRESULT> {
        println!("RigX::FrequencyOfTone called with tone: {}", tone);
        let mode = self.inner.mode();
        let mut result = tone;
        if mode == RigParamX::CwU || mode == RigParamX::CwL {
            result -= self.inner.pitch();
        }
        if mode == RigParamX::CwL || mode == RigParamX::SsbL {
            result = -result;
        }
        result += self.inner.freq();
        Ok(result)
    }

    #[id(0x17)]
    fn SendCustomCommand(
        &self,
        command: VARIANT,
        reply_length: i32,
        reply_end: VARIANT,
    ) -> Result<(), HRESULT> {
        println!("RigX::SendCustomCommand called with reply_length: {reply_length}");
        let command_bytes = variant_to_bytes(&command)?;
        let reply_end_bytes = variant_to_bytes(&reply_end)?;
        self.inner
            .send_custom_command(&command_bytes, reply_length, &reply_end_bytes);
        Ok(())
    }

    #[id(0x18)]
    fn GetRxFrequency(&self) -> Result<i32, HRESULT> {
        println!("RigX::GetRxFrequency called");
        let vfo = self.inner.vfo();

        let mut result = match vfo {
            RigParamX::VfoA | RigParamX::VfoAA | RigParamX::VfoAB => self.inner.freq_a(),
            RigParamX::VfoB | RigParamX::VfoBA | RigParamX::VfoBB => self.inner.freq_b(),
            _ => {
                if self.inner.tx() != RigParamX::Tx || self.inner.split() != RigParamX::SplitOn {
                    self.inner.freq()
                } else {
                    0
                }
            }
        };

        if self.inner.rit() == RigParamX::RitOn {
            result += self.inner.rit_offset();
        }
        Ok(result)
    }

    #[id(0x19)]
    fn GetTxFrequency(&self) -> Result<i32, HRESULT> {
        println!("RigX::GetTxFrequency called");
        let vfo = self.inner.vfo();
        let split = self.inner.split();

        let mut result = match vfo {
            RigParamX::VfoAA | RigParamX::VfoBA => self.inner.freq_a(),
            RigParamX::VfoAB | RigParamX::VfoBB => self.inner.freq_b(),
            RigParamX::VfoA if split == RigParamX::SplitOff => self.inner.freq_a(),
            RigParamX::VfoA if split == RigParamX::SplitOn => self.inner.freq_b(),
            RigParamX::VfoB if split == RigParamX::SplitOff => self.inner.freq_b(),
            RigParamX::VfoB if split == RigParamX::SplitOn => self.inner.freq_a(),
            _ => {
                if self.inner.tx() == RigParamX::Tx {
                    self.inner.freq()
                } else {
                    0
                }
            }
        };

        if self.inner.xit() == RigParamX::XitOn {
            result += self.inner.rit_offset();
        }
        Ok(result)
    }

    #[id(0x1A)]
    #[getter]
    fn PortBits(&self) -> Result<IDispatch, HRESULT> {
        println!("RigX::PortBits getter called");
        let port_bits = self
            .port_bits_com
            .read()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or(windows::Win32::Foundation::E_FAIL)?;
        Ok(port_bits.cast()?)
    }
}

// Manual IRigX_Impl implementation to bridge COM interface with auto_dispatch methods
impl crate::rig::IRigX_Impl for RigX_Impl {
    unsafe fn get_RigType(&self, value: *mut BSTR) -> HRESULT {
        match self.get_RigType() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_ReadableParams(&self, value: *mut i32) -> HRESULT {
        match self.get_ReadableParams() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_WriteableParams(&self, value: *mut i32) -> HRESULT {
        match self.get_WriteableParams() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn IsParamReadable(&self, Param: i32, value: *mut bool) -> HRESULT {
        match self.IsParamReadable(Param) {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn IsParamWriteable(&self, Param: i32, value: *mut bool) -> HRESULT {
        match self.IsParamWriteable(Param) {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_Status(&self, value: *mut i32) -> HRESULT {
        match self.get_Status() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_StatusStr(&self, value: *mut BSTR) -> HRESULT {
        match self.get_StatusStr() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_Freq(&self, value: *mut i32) -> HRESULT {
        match self.get_Freq() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Freq(&self, value: i32) -> HRESULT {
        match self.set_Freq(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_FreqA(&self, value: *mut i32) -> HRESULT {
        match self.get_FreqA() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_FreqA(&self, value: i32) -> HRESULT {
        match self.set_FreqA(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_FreqB(&self, value: *mut i32) -> HRESULT {
        match self.get_FreqB() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_FreqB(&self, value: i32) -> HRESULT {
        match self.set_FreqB(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_RitOffset(&self, value: *mut i32) -> HRESULT {
        match self.get_RitOffset() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_RitOffset(&self, value: i32) -> HRESULT {
        match self.set_RitOffset(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Pitch(&self, value: *mut i32) -> HRESULT {
        match self.get_Pitch() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Pitch(&self, value: i32) -> HRESULT {
        match self.set_Pitch(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Vfo(&self, value: *mut i32) -> HRESULT {
        match self.get_Vfo() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Vfo(&self, value: i32) -> HRESULT {
        match self.set_Vfo(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Split(&self, value: *mut i32) -> HRESULT {
        match self.get_Split() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Split(&self, value: i32) -> HRESULT {
        match self.set_Split(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Rit(&self, value: *mut i32) -> HRESULT {
        match self.get_Rit() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Rit(&self, value: i32) -> HRESULT {
        match self.set_Rit(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Xit(&self, value: *mut i32) -> HRESULT {
        match self.get_Xit() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Xit(&self, value: i32) -> HRESULT {
        match self.set_Xit(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Tx(&self, value: *mut i32) -> HRESULT {
        match self.get_Tx() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Tx(&self, value: i32) -> HRESULT {
        match self.set_Tx(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Mode(&self, value: *mut i32) -> HRESULT {
        match self.get_Mode() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Mode(&self, value: i32) -> HRESULT {
        match self.set_Mode(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn ClearRit(&self) -> HRESULT {
        match self.ClearRit() {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn SetSimplexMode(&self, Freq: i32) -> HRESULT {
        match self.SetSimplexMode(Freq) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn SetSplitMode(&self, RxFreq: i32, TxFreq: i32) -> HRESULT {
        match self.SetSplitMode(RxFreq, TxFreq) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn FrequencyOfTone(&self, Tone: i32, value: *mut i32) -> HRESULT {
        match self.FrequencyOfTone(Tone) {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn GetRxFrequency(&self, value: *mut i32) -> HRESULT {
        match self.GetRxFrequency() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn GetTxFrequency(&self, value: *mut i32) -> HRESULT {
        match self.GetTxFrequency() {
            Ok(v) => {
                *value = v;
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_PortBits(&self, value: *mut Option<IDispatch>) -> HRESULT {
        match self.get_PortBits() {
            Ok(v) => {
                *value = Some(v);
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn SendCustomCommand(
        &self,
        Command: VARIANT,
        ReplyLength: i32,
        ReplyEnd: VARIANT,
    ) -> HRESULT {
        match self.SendCustomCommand(Command, ReplyLength, ReplyEnd) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }
}
