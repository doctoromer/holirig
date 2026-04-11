#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::{Arc, RwLock};
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_NOINTERFACE};
use windows::Win32::System::Com::{
    IClassFactory, IClassFactory_Impl, IDispatch, IDispatch_Impl, IDispatch_Vtbl,
};
use windows::Win32::System::Com::{
    IConnectionPoint, IConnectionPointContainer, IConnectionPointContainer_Impl,
    IEnumConnectionPoints,
};
use windows::Win32::System::Ole::CONNECT_E_NOCONNECTION;
use windows::core::{GUID, IUnknown, Interface, implement};
use windows_core::{BOOL, HRESULT, interface};

use tracing::{debug, trace};

use crate::EventDispatcher;
use crate::connection_point::{EventSinks, OMNIRIG_EVENTS_IID, OmniRigEventsConnectionPoint};
use crate::provider::OmniRigProvider;
use crate::rig::{IRigX, RigX};
use auto_dispatch::auto_dispatch;

#[interface("501A2858-3331-467A-837A-989FDEDACC7D")]
pub unsafe trait IOmniRigX: IDispatch {
    pub fn get_InterfaceVersion(&self, Value: *mut i32) -> HRESULT;
    pub fn get_SoftwareVersion(&self, Value: *mut i32) -> HRESULT;
    pub fn get_Rig1(&self, Value: *mut Option<IRigX>) -> HRESULT;
    pub fn get_Rig2(&self, Value: *mut Option<IRigX>) -> HRESULT;
    pub fn get_DialogVisible(&self, Value: *mut bool) -> HRESULT;
    pub fn set_DialogVisible(&self, Value: bool) -> HRESULT;
}

#[implement(IOmniRigX, IConnectionPointContainer)]
pub struct OmniRigX {
    dialog_visible: RwLock<bool>,
    rig1: RwLock<Option<IRigX>>,
    rig2: RwLock<Option<IRigX>>,
    connection_point: IConnectionPoint,
}

impl OmniRigX {
    pub fn from_provider(provider: &dyn OmniRigProvider, event_sinks: Arc<EventSinks>) -> Self {
        let rig1: IRigX = RigX::new(provider.create_rig1()).into();
        let rig2: IRigX = RigX::new(provider.create_rig2()).into();
        let cp: IConnectionPoint = OmniRigEventsConnectionPoint { sinks: event_sinks }.into();

        Self {
            dialog_visible: RwLock::new(false),
            rig1: RwLock::new(Some(rig1)),
            rig2: RwLock::new(Some(rig2)),
            connection_point: cp,
        }
    }
}

impl IConnectionPointContainer_Impl for OmniRigX_Impl {
    fn EnumConnectionPoints(&self) -> windows::core::Result<IEnumConnectionPoints> {
        Ok(
            crate::connection_point::EnumConnectionPoints::new(vec![self.connection_point.clone()])
                .into(),
        )
    }

    fn FindConnectionPoint(&self, riid: *const GUID) -> windows::core::Result<IConnectionPoint> {
        if unsafe { *riid } == OMNIRIG_EVENTS_IID {
            Ok(self.connection_point.clone())
        } else {
            Err(windows::core::Error::from_hresult(CONNECT_E_NOCONNECTION))
        }
    }
}

#[auto_dispatch(type_info = crate::typelib::omnirigx_type_info)]
impl OmniRigX {
    #[id(0x01)]
    #[getter]
    fn InterfaceVersion(&self) -> Result<i32, HRESULT> {
        trace!("OmniRigX::InterfaceVersion getter called");
        Ok(0x101)
    }

    #[id(0x02)]
    #[getter]
    fn SoftwareVersion(&self) -> Result<i32, HRESULT> {
        trace!("OmniRigX::SoftwareVersion getter called");
        Ok(0x10014)
    }

    #[id(0x03)]
    #[getter]
    fn Rig1(&self) -> Result<IDispatch, HRESULT> {
        trace!("OmniRigX::Rig1 getter called");
        let rig = self
            .rig1
            .read()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or(windows::Win32::Foundation::E_FAIL)?;
        Ok(rig.cast()?)
    }

    #[id(0x04)]
    #[getter]
    fn Rig2(&self) -> Result<IDispatch, HRESULT> {
        trace!("OmniRigX::Rig2 getter called");
        let rig = self
            .rig2
            .read()
            .unwrap()
            .as_ref()
            .cloned()
            .ok_or(windows::Win32::Foundation::E_FAIL)?;
        Ok(rig.cast()?)
    }

    #[id(0x05)]
    #[getter]
    fn DialogVisible(&self) -> Result<bool, HRESULT> {
        trace!("OmniRigX::DialogVisible getter called");
        Ok(*self.dialog_visible.read().unwrap())
    }

    #[id(0x05)]
    #[setter]
    fn DialogVisible(&self, value: bool) -> Result<(), HRESULT> {
        trace!(value, "OmniRigX::DialogVisible setter called");
        *self.dialog_visible.write().unwrap() = value;
        Ok(())
    }
}

impl IOmniRigX_Impl for OmniRigX_Impl {
    unsafe fn get_InterfaceVersion(&self, value: *mut i32) -> HRESULT {
        unsafe {
            *value = self.get_InterfaceVersion().unwrap();
        }
        HRESULT(0)
    }
    unsafe fn get_SoftwareVersion(&self, value: *mut i32) -> HRESULT {
        unsafe {
            *value = self.get_SoftwareVersion().unwrap();
        }
        HRESULT(0)
    }
    unsafe fn get_Rig1(&self, value: *mut Option<IRigX>) -> HRESULT {
        let disp = self.get_Rig1().unwrap();
        unsafe {
            *value = Some(disp.cast().unwrap());
        }
        HRESULT(0)
    }
    unsafe fn get_Rig2(&self, value: *mut Option<IRigX>) -> HRESULT {
        let disp = self.get_Rig2().unwrap();
        unsafe {
            *value = Some(disp.cast().unwrap());
        }
        HRESULT(0)
    }
    unsafe fn get_DialogVisible(&self, value: *mut bool) -> HRESULT {
        unsafe {
            *value = self.get_DialogVisible().unwrap();
        }
        HRESULT(0)
    }
    unsafe fn set_DialogVisible(&self, value: bool) -> HRESULT {
        self.set_DialogVisible(value).unwrap();
        HRESULT(0)
    }
}

#[implement(IClassFactory)]
pub struct OmniRigXFactory {
    provider: Arc<dyn OmniRigProvider>,
    dispatcher: EventDispatcher,
}

impl OmniRigXFactory {
    pub fn new(provider: Arc<dyn OmniRigProvider>, dispatcher: EventDispatcher) -> Self {
        Self {
            provider,
            dispatcher,
        }
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl IClassFactory_Impl for OmniRigXFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: windows_core::Ref<IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if punkouter.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }

        unsafe {
            let requested_iid = *riid;

            if requested_iid != IUnknown::IID
                && requested_iid != IDispatch::IID
                && requested_iid != IConnectionPointContainer::IID
            {
                *ppvobject = std::ptr::null_mut();
                return Err(E_NOINTERFACE.into());
            }

            debug!("OmniRigXFactory: Creating new OmniRigX instance");
            let event_sinks = EventSinks::new();
            self.dispatcher.register_sinks(Arc::clone(&event_sinks));
            let instance: IOmniRigX =
                OmniRigX::from_provider(self.provider.as_ref(), event_sinks).into();
            *ppvobject = std::mem::transmute_copy(&instance);
            std::mem::forget(instance);
        }
        Ok(())
    }

    fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}
