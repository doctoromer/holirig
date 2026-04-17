#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use windows::Win32::System::Com::{IDispatch, IDispatch_Impl, IDispatch_Vtbl};
use windows::core::implement;
use windows_core::{HRESULT, interface};

use tracing::trace;

use auto_dispatch::auto_dispatch;

use crate::provider::PortBitsControl;

#[interface("3DEE2CC8-1EA3-46E7-B8B4-3E7321F2446A")]
pub unsafe trait IPortBits: IDispatch {
    fn Lock(&self, ok: *mut bool) -> HRESULT;
    fn get_Rts(&self, Value: *mut bool) -> HRESULT;
    fn put_Rts(&self, Value: bool) -> HRESULT;
    fn get_Dtr(&self, Value: *mut bool) -> HRESULT;
    fn put_Dtr(&self, Value: bool) -> HRESULT;
    fn get_Cts(&self, Value: *mut bool) -> HRESULT;
    fn get_Dsr(&self, Value: *mut bool) -> HRESULT;
    fn Unlock(&self) -> HRESULT;
}

#[implement(IPortBits)]
pub struct PortBits {
    inner: Box<dyn PortBitsControl>,
}

impl PortBits {
    pub fn new(inner: Box<dyn PortBitsControl>) -> Self {
        Self { inner }
    }
}

#[auto_dispatch]
impl PortBits {
    #[id(0x01)]
    fn Lock(&self) -> Result<bool, HRESULT> {
        let value = self.inner.lock();
        trace!(value, "PortBits::Lock called");
        Ok(value)
    }

    #[id(0x02)]
    #[getter]
    fn Rts(&self) -> Result<bool, HRESULT> {
        let value = self.inner.rts();
        trace!(value, "PortBits::Rts getter called");
        Ok(value)
    }

    #[id(0x02)]
    #[setter]
    fn Rts(&self, value: bool) -> Result<(), HRESULT> {
        trace!(value, "PortBits::Rts setter called");
        self.inner.set_rts(value);
        Ok(())
    }

    #[id(0x03)]
    #[getter]
    fn Dtr(&self) -> Result<bool, HRESULT> {
        let value = self.inner.dtr();
        trace!(value, "PortBits::Dtr getter called");
        Ok(value)
    }

    #[id(0x03)]
    #[setter]
    fn Dtr(&self, value: bool) -> Result<(), HRESULT> {
        trace!(value, "PortBits::Dtr setter called");
        self.inner.set_dtr(value);
        Ok(())
    }

    #[id(0x04)]
    #[getter]
    fn Cts(&self) -> Result<bool, HRESULT> {
        let value = self.inner.cts();
        trace!(value, "PortBits::Cts getter called");
        Ok(value)
    }

    #[id(0x05)]
    #[getter]
    fn Dsr(&self) -> Result<bool, HRESULT> {
        let value = self.inner.dsr();
        trace!(value, "PortBits::Dsr getter called");
        Ok(value)
    }

    #[id(0x06)]
    fn Unlock(&self) -> Result<(), HRESULT> {
        trace!("PortBits::Unlock called");
        self.inner.unlock();
        Ok(())
    }
}

// Manual IPortBits_Impl implementation to bridge COM interface with auto_dispatch methods
impl crate::port_bits::IPortBits_Impl for PortBits_Impl {
    unsafe fn Lock(&self, ok: *mut bool) -> HRESULT {
        match self.Lock() {
            Ok(v) => {
                unsafe { *ok = v };
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_Rts(&self, value: *mut bool) -> HRESULT {
        match self.get_Rts() {
            Ok(v) => {
                unsafe { *value = v };
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Rts(&self, value: bool) -> HRESULT {
        match self.set_Rts(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Dtr(&self, value: *mut bool) -> HRESULT {
        match self.get_Dtr() {
            Ok(v) => {
                unsafe { *value = v };
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn put_Dtr(&self, value: bool) -> HRESULT {
        match self.set_Dtr(value) {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }

    unsafe fn get_Cts(&self, value: *mut bool) -> HRESULT {
        match self.get_Cts() {
            Ok(v) => {
                unsafe { *value = v };
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn get_Dsr(&self, value: *mut bool) -> HRESULT {
        match self.get_Dsr() {
            Ok(v) => {
                unsafe { *value = v };
                HRESULT(0)
            }
            Err(e) => e,
        }
    }

    unsafe fn Unlock(&self) -> HRESULT {
        match self.Unlock() {
            Ok(_) => HRESULT(0),
            Err(e) => e,
        }
    }
}
