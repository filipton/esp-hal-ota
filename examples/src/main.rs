#![no_std]
#![no_main]

extern crate alloc;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::{Config, Runner, StackResources, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::timer::timg::TimerGroup;
use esp_hal_ota::Ota;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig, Interface, WifiController,
    sta::StationConfig,
};
use esp_storage::FlashStorage;
use static_cell::StaticCell;

const WIFI_SSID: &str = env!("SSID");
const WIFI_PSK: &str = env!("PSK");
const OTA_SERVER_IP: &str = env!("OTA_IP");

const RX_BUFFER_SIZE: usize = 16384;
const TX_BUFFER_SIZE: usize = 16384;
static RX_BUFF: StaticCell<[u8; RX_BUFFER_SIZE]> = StaticCell::new();
static TX_BUFF: StaticCell<[u8; TX_BUFFER_SIZE]> = StaticCell::new();

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    #[cfg(not(feature = "esp32"))]
    {
        esp_alloc::heap_allocator!(size: 150 * 1024);
    }

    #[cfg(feature = "esp32")]
    {
        // esp32 has little contiguous DRAM: use the RAM reclaimed from the bootloader too
        esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 96 * 1024);
        esp_alloc::heap_allocator!(size: 24 * 1024);
    }

    let mut peripherals = esp_hal::init(esp_hal::Config::default());

    esp_println::logger::init_logger_from_env();

    let timg0 = TimerGroup::new(peripherals.TIMG0);

    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    let station_config = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(WIFI_SSID.try_into().expect("Wifi ssid parse"))
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                WIFI_PSK.try_into().expect("Wifi psk parse"),
            )),
    );
    let wifi_interface = Interface::station();
    let controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .unwrap();

    let config = Config::dhcpv4(Default::default());
    let seed = 69420;

    let (stack, runner) = embassy_net::new(
        wifi_interface,
        config,
        {
            static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
            RESOURCES.init(StackResources::new())
        },
        seed,
    );

    spawner.spawn(connection(controller).expect("connection spawn"));
    spawner.spawn(net_task(runner).expect("net task spawn"));

    // mark ota partition valid
    {
        let mut ota =
            Ota::new(FlashStorage::new(peripherals.FLASH.reborrow())).expect("Cannot create ota");
        _ = ota.mark_app_valid(); //do not unwrap here if using factory/test partition
    }

    loop {
        log::info!("Wait for wifi!");
        Timer::after(Duration::from_secs(1)).await;

        if let Some(config) = stack.config_v4() {
            log::info!("Got IP: {}", config.address);
            break;
        }
    }

    let mut socket = TcpSocket::new(
        stack,
        RX_BUFF.init([0; RX_BUFFER_SIZE]),
        TX_BUFF.init([0; TX_BUFFER_SIZE]),
    );

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

    let mut ota = Ota::new(FlashStorage::new(peripherals.FLASH)).expect("Cannot create ota");
    ota.begin(flash_size, target_crc).unwrap();

    let mut bytes_read: usize = 0;
    loop {
        let bytes_to_read = 8192.min(flash_size - bytes_read as u32);
        let res = read_exact(&mut socket, &mut ota_buff[..bytes_to_read as usize]).await;
        if res.is_ok() {
            bytes_read += bytes_to_read as usize;
            if bytes_to_read == 0 {
                break;
            }

            let res = ota.write(&ota_buff[..bytes_to_read as usize]);
            _ = socket.write(&[0]).await;

            match res {
                Ok(true) => {
                    let res = ota.finish(true);
                    if let Err(e) = res {
                        log::error!("Ota finish error: {e:?}");
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

        if let Some(progress) = ota.progress() {
            log::info!("Progress: {}%", progress.percent());
        }
    }

    loop {
        log::info!("bump");
        Timer::after_millis(15000).await;
    }
}

#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    log::info!("start connection task");
    loop {
        log::info!("About to connect...");

        match controller.connect_async().await {
            Ok(_) => {
                log::info!("Wifi connected!");

                // wait until we're no longer connected
                _ = controller.wait_for_disconnect_async().await;
                Timer::after(Duration::from_millis(5000)).await
            }
            Err(e) => {
                log::info!("Failed to connect to wifi: {e:?}");
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, Interface>) {
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
