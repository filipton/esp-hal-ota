#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

extern crate alloc;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::{Config, Runner, Stack, StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_hal_ota::Ota;
use esp_storage::FlashStorage;
use esp_wifi::{
    EspWifiController,
    wifi::{ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiState},
};

const WIFI_SSID: &'static str = env!("SSID");
const WIFI_PSK: &'static str = env!("PSK");
const OTA_SERVER_IP: &'static str = env!("OTA_IP");

const RX_BUFFER_SIZE: usize = 16384;
const TX_BUFFER_SIZE: usize = 16384;
static mut TX_BUFF: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];
static mut RX_BUFF: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];

macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_hal_embassy::main]
async fn main(spawner: Spawner) {
    #[cfg(not(feature = "esp32"))]
    {
        esp_alloc::heap_allocator!(size: 150 * 1024);
    }

    #[cfg(feature = "esp32")]
    {
        static mut HEAP: core::mem::MaybeUninit<[u8; 30 * 1024]> = core::mem::MaybeUninit::uninit();

        #[unsafe(link_section = ".dram2_uninit")]
        static mut HEAP2: core::mem::MaybeUninit<[u8; 64 * 1024]> =
            core::mem::MaybeUninit::uninit();

        unsafe {
            #[allow(static_mut_refs)]
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                HEAP.as_mut_ptr() as *mut u8,
                core::mem::size_of_val(&*core::ptr::addr_of!(HEAP)),
                esp_alloc::MemoryCapability::Internal.into(),
            ));

            #[allow(static_mut_refs)]
            esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
                HEAP2.as_mut_ptr() as *mut u8,
                core::mem::size_of_val(&*core::ptr::addr_of!(HEAP2)),
                esp_alloc::MemoryCapability::Internal.into(),
            ));
        }
    }

    let peripherals = esp_hal::init(esp_hal::Config::default());
    //let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    esp_println::logger::init_logger_from_env();
    //log::set_max_level(log::LevelFilter::Info); // only for esp32s3??

    let rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timg1 = TimerGroup::new(peripherals.TIMG1);

    let init = &*mk_static!(
        EspWifiController<'static>,
        esp_wifi::init(timg1.timer0, rng.clone()).unwrap()
    );

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timg0.timer0);

    let wifi = peripherals.WIFI;
    let (controller, interfaces) = esp_wifi::wifi::new(&init, wifi).unwrap();

    let config = Config::dhcpv4(Default::default());
    let seed = 69420;

    let (stack, runner) = embassy_net::new(
        interfaces.sta,
        config,
        {
            static STATIC_CELL: static_cell::StaticCell<StackResources<3>> =
                static_cell::StaticCell::new();
            STATIC_CELL.uninit().write(StackResources::<3>::new())
        },
        seed,
    );

    spawner
        .spawn(connection(controller, stack))
        .expect("connection spawn");
    spawner.spawn(net_task(runner)).expect("net task spawn");

    // mark ota partition valid
    {
        let mut ota = Ota::new(FlashStorage::new()).expect("Cannot create ota");
        _ = ota.ota_mark_app_valid(); //do not unwrap here if using factory/test partition
    }

    loop {
        log::info!("Wait for wifi!");
        Timer::after(Duration::from_secs(1)).await;

        if let Some(config) = stack.config_v4() {
            log::info!("Got IP: {}", config.address);
            break;
        }
    }

    let mut socket = unsafe {
        TcpSocket::new(
            stack,
            &mut *core::ptr::addr_of_mut!(RX_BUFF),
            &mut *core::ptr::addr_of_mut!(TX_BUFF),
        )
    };

    let ip = embassy_net::IpEndpoint::from_str(OTA_SERVER_IP).expect("Wrong ip addr");
    socket.connect(ip).await.expect("Cannot connect!");
    let mut ota_buff = [0; 4096 * 2];
    socket
        .read(&mut ota_buff[..4])
        .await
        .expect("Cannot read firmware size!");
    let flash_size = u32::from_le_bytes(ota_buff[..4].try_into().unwrap());

    socket
        .read(&mut ota_buff[..4])
        .await
        .expect("Cannot read target crc!");
    let target_crc = u32::from_le_bytes(ota_buff[..4].try_into().unwrap());

    log::info!("flash_size: {flash_size}");
    log::info!("target_crc: {target_crc}");

    let mut ota = Ota::new(FlashStorage::new()).expect("Cannot create ota");
    ota.ota_begin(flash_size, target_crc).unwrap();

    let mut bytes_read: usize = 0;
    loop {
        let bytes_to_read = 8192.min(flash_size - bytes_read as u32);
        let res = read_exact(&mut socket, &mut ota_buff[..bytes_to_read as usize]).await;
        if let Ok(_) = res {
            bytes_read += bytes_to_read as usize;
            if bytes_to_read == 0 {
                break;
            }

            let res = ota.ota_write_chunk(&ota_buff[..bytes_to_read as usize]);
            _ = socket.write(&[0]).await;

            match res {
                Ok(true) => {
                    let res = ota.ota_flush(false, true);
                    if let Err(e) = res {
                        log::error!("Ota flush error: {e:?}");
                        break;
                    }

                    log::info!("Ota OK! Rebooting in 1s!");
                    Timer::after_millis(1000).await;
                    esp_hal::system::software_reset();
                }
                Err(e) => {
                    log::error!("Ota write error: {e:?}");
                    break;
                }
                _ => {}
            }
        }

        log::info!("Progress: {}%", (ota.get_ota_progress() * 100.0) as u8);
    }

    loop {
        log::info!("bump");
        Timer::after_millis(15000).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>, stack: Stack<'static>) {
    log::info!("start connection task");
    log::info!("Device capabilities: {:?}", controller.capabilities());
    loop {
        if esp_wifi::wifi::wifi_state() == WifiState::StaConnected {
            // wait until we're no longer connected
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await
        }
        if !matches!(controller.is_started(), Ok(true)) {
            let client_config = Configuration::Client(ClientConfiguration {
                ssid: WIFI_SSID.try_into().expect("Wifi ssid parse"),
                password: WIFI_PSK.try_into().expect("Wifi psk parse"),
                ..Default::default()
            });
            controller.set_configuration(&client_config).unwrap();
            log::info!("Starting wifi");
            controller.start_async().await.unwrap();
            log::info!("Wifi started!");
        }
        log::info!("About to connect...");

        match controller.connect_async().await {
            Ok(_) => {
                log::info!("Wifi connected!");

                loop {
                    if stack.is_link_up() {
                        break;
                    }
                    Timer::after(Duration::from_millis(500)).await;
                }
            }
            Err(e) => {
                log::info!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}

pub async fn read_exact(
    socket: &mut TcpSocket<'_>,
    mut buf: &mut [u8],
) -> Result<(), embassy_net::tcp::Error> {
    while !buf.is_empty() {
        match socket.read(buf).await {
            Ok(0) => return Err(embassy_net::tcp::Error::ConnectionReset),
            Ok(n) => {
                buf = &mut buf[n..];
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
