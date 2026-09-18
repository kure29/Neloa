use tauri::AppHandle;
use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

use crate::model::PeerDevice;

#[derive(Clone)]
pub(crate) struct BluetoothHandle {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    shared: std::sync::Arc<apple::Shared>,
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
impl Default for BluetoothHandle {
    fn default() -> Self {
        Self {}
    }
}

impl BluetoothHandle {
    pub(crate) async fn connect(
        &self,
        _peer: &PeerDevice,
    ) -> Result<(WriteHalf<DuplexStream>, ReadHalf<DuplexStream>), String> {
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        {
            self.shared.ensure_ready().await?;
            Err(
                "蓝牙已获得授权并可用，但 GATT 数据通道仍在实现中；Neloa 不会改用其他连接方式"
                    .into(),
            )
        }

        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        {
            Err("蓝牙传输尚未在当前平台启用；Neloa 不会改用其他连接方式".into())
        }
    }
}

pub(crate) fn start(app: AppHandle) -> BluetoothHandle {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        BluetoothHandle {
            shared: apple::Shared::new(app),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = app;
        BluetoothHandle::default()
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use std::{
        cell::RefCell,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    };

    use objc2::{
        define_class, msg_send, rc::Retained, runtime::ProtocolObject, AnyThread, DefinedClass,
        MainThreadOnly,
    };
    use objc2_core_bluetooth::{
        CBCentralManager, CBCentralManagerDelegate, CBManagerState, CBPeripheralManager,
        CBPeripheralManagerDelegate,
    };
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol};
    use parking_lot::RwLock;
    use tauri::AppHandle;
    use tokio::{
        sync::Notify,
        time::{timeout_at, Instant},
    };

    const AVAILABILITY_TIMEOUT: Duration = Duration::from_secs(30);

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    enum ManagerState {
        #[default]
        Unknown,
        Resetting,
        Unsupported,
        Unauthorized,
        PoweredOff,
        PoweredOn,
    }

    impl From<CBManagerState> for ManagerState {
        fn from(state: CBManagerState) -> Self {
            match state {
                CBManagerState::Resetting => Self::Resetting,
                CBManagerState::Unsupported => Self::Unsupported,
                CBManagerState::Unauthorized => Self::Unauthorized,
                CBManagerState::PoweredOff => Self::PoweredOff,
                CBManagerState::PoweredOn => Self::PoweredOn,
                _ => Self::Unknown,
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    struct Availability {
        central: ManagerState,
        peripheral: ManagerState,
        startup_error: Option<String>,
    }

    impl Availability {
        fn outcome(&self) -> Option<Result<(), String>> {
            if let Some(error) = &self.startup_error {
                return Some(Err(error.clone()));
            }
            if self.central == ManagerState::Unsupported
                || self.peripheral == ManagerState::Unsupported
            {
                return Some(Err("这台 Apple 设备不支持低功耗蓝牙传输".into()));
            }
            if self.central == ManagerState::Unauthorized
                || self.peripheral == ManagerState::Unauthorized
            {
                return Some(Err(
                    "蓝牙访问被拒绝，请在系统设置的隐私与安全性中允许 Neloa 使用蓝牙".into(),
                ));
            }
            if self.central == ManagerState::PoweredOff
                || self.peripheral == ManagerState::PoweredOff
            {
                return Some(Err("蓝牙已关闭，请先在系统设置中打开蓝牙".into()));
            }
            if self.central == ManagerState::PoweredOn && self.peripheral == ManagerState::PoweredOn
            {
                return Some(Ok(()));
            }
            None
        }
    }

    pub(super) struct Shared {
        app: AppHandle,
        started: AtomicBool,
        availability: RwLock<Availability>,
        changed: Notify,
    }

    impl Shared {
        pub(super) fn new(app: AppHandle) -> Arc<Self> {
            Arc::new(Self {
                app,
                started: AtomicBool::new(false),
                availability: RwLock::new(Availability::default()),
                changed: Notify::new(),
            })
        }

        pub(super) async fn ensure_ready(self: &Arc<Self>) -> Result<(), String> {
            if let Some(outcome) = self.availability.read().outcome() {
                return outcome;
            }
            self.start_managers()?;

            let deadline = Instant::now() + AVAILABILITY_TIMEOUT;
            loop {
                let changed = self.changed.notified();
                if let Some(outcome) = self.availability.read().outcome() {
                    return outcome;
                }
                timeout_at(deadline, changed).await.map_err(|_| {
                    "等待蓝牙授权或硬件状态超时；请检查系统蓝牙权限后重试".to_string()
                })?;
            }
        }

        fn start_managers(self: &Arc<Self>) -> Result<(), String> {
            if self
                .started
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Ok(());
            }

            let shared = Arc::clone(self);
            if let Err(error) = self.app.run_on_main_thread(move || {
                let Some(marker) = MainThreadMarker::new() else {
                    shared.fail_startup("无法在主线程初始化 Apple 蓝牙服务".into());
                    return;
                };
                APPLE_RUNTIME.with_borrow_mut(|runtime| {
                    if runtime.is_none() {
                        *runtime = Some(AppleRuntime::new(marker, shared));
                    }
                });
            }) {
                self.started.store(false, Ordering::Release);
                return Err(format!("无法调度 Apple 蓝牙初始化：{error}"));
            }
            Ok(())
        }

        fn update_central(&self, state: CBManagerState) {
            self.availability.write().central = state.into();
            self.changed.notify_waiters();
        }

        fn update_peripheral(&self, state: CBManagerState) {
            self.availability.write().peripheral = state.into();
            self.changed.notify_waiters();
        }

        fn fail_startup(&self, error: String) {
            self.availability.write().startup_error = Some(error);
            self.changed.notify_waiters();
        }
    }

    struct BluetoothDelegateIvars {
        shared: Arc<Shared>,
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements and the delegate is
        // retained for exactly as long as both CoreBluetooth managers.
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = BluetoothDelegateIvars]
        struct BluetoothDelegate;

        // SAFETY: NSObjectProtocol has no additional implementation invariants.
        unsafe impl NSObjectProtocol for BluetoothDelegate {}

        // SAFETY: CoreBluetooth invokes this delegate on the main queue selected
        // when the manager is created, matching the class thread kind.
        unsafe impl CBCentralManagerDelegate for BluetoothDelegate {
            #[unsafe(method(centralManagerDidUpdateState:))]
            fn central_manager_did_update_state(&self, central: &CBCentralManager) {
                // SAFETY: The manager is supplied by CoreBluetooth for the
                // duration of this delegate callback.
                let state = unsafe { central.state() };
                self.ivars().shared.update_central(state);
            }
        }

        // SAFETY: CoreBluetooth invokes this delegate on the main queue selected
        // when the manager is created, matching the class thread kind.
        unsafe impl CBPeripheralManagerDelegate for BluetoothDelegate {
            #[unsafe(method(peripheralManagerDidUpdateState:))]
            fn peripheral_manager_did_update_state(&self, peripheral: &CBPeripheralManager) {
                // SAFETY: The manager is supplied by CoreBluetooth for the
                // duration of this delegate callback.
                let state = unsafe { peripheral.state() };
                self.ivars().shared.update_peripheral(state);
            }
        }
    );

    impl BluetoothDelegate {
        fn new(marker: MainThreadMarker, shared: Arc<Shared>) -> Retained<Self> {
            let this = Self::alloc(marker).set_ivars(BluetoothDelegateIvars { shared });
            // SAFETY: This calls NSObject's designated initializer on a newly
            // allocated BluetoothDelegate.
            unsafe { msg_send![super(this), init] }
        }
    }

    struct AppleRuntime {
        _delegate: Retained<BluetoothDelegate>,
        _central: Retained<CBCentralManager>,
        _peripheral: Retained<CBPeripheralManager>,
    }

    impl AppleRuntime {
        fn new(marker: MainThreadMarker, shared: Arc<Shared>) -> Self {
            let delegate = BluetoothDelegate::new(marker, shared);
            let central_delegate = ProtocolObject::from_ref(&*delegate);
            let peripheral_delegate = ProtocolObject::from_ref(&*delegate);

            // SAFETY: Both managers and their weak delegate are created and
            // retained together on the main thread; a nil queue selects the
            // main dispatch queue for all callbacks.
            let central = unsafe {
                CBCentralManager::initWithDelegate_queue(
                    CBCentralManager::alloc(),
                    Some(central_delegate),
                    None,
                )
            };
            // SAFETY: Same lifetime and queue guarantees as the central manager.
            let peripheral = unsafe {
                CBPeripheralManager::initWithDelegate_queue(
                    CBPeripheralManager::alloc(),
                    Some(peripheral_delegate),
                    None,
                )
            };

            Self {
                _delegate: delegate,
                _central: central,
                _peripheral: peripheral,
            }
        }
    }

    thread_local! {
        static APPLE_RUNTIME: RefCell<Option<AppleRuntime>> = const { RefCell::new(None) };
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn availability_requires_both_core_bluetooth_roles() {
            let mut availability = Availability {
                central: ManagerState::PoweredOn,
                peripheral: ManagerState::Unknown,
                startup_error: None,
            };
            assert!(availability.outcome().is_none());
            availability.peripheral = ManagerState::PoweredOn;
            assert_eq!(availability.outcome(), Some(Ok(())));
        }

        #[test]
        fn availability_reports_actionable_failures() {
            let denied = Availability {
                central: ManagerState::Unauthorized,
                peripheral: ManagerState::PoweredOn,
                startup_error: None,
            };
            assert!(denied
                .outcome()
                .unwrap()
                .unwrap_err()
                .contains("隐私与安全性"));

            let powered_off = Availability {
                central: ManagerState::PoweredOn,
                peripheral: ManagerState::PoweredOff,
                startup_error: None,
            };
            assert!(powered_off
                .outcome()
                .unwrap()
                .unwrap_err()
                .contains("打开蓝牙"));
        }
    }
}
