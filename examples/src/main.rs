#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_net::{tcp::TcpSocket, Config, Stack, StackResources};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::{
    clock::ClockControl, peripherals::Peripherals, prelude::*, system::SystemControl,
    timer::timg::TimerGroup,
};
use esp_hal_ota::Ota;
use esp_storage::FlashStorage;
use esp_wifi::wifi::{
    ClientConfiguration, Configuration, WifiController, WifiDevice, WifiEvent, WifiStaDevice,
    WifiState,
};
use static_cell::make_static;

const WIFI_SSID: &'static str = env!("SSID");
const WIFI_PSK: &'static str = env!("PSK");
const OTA_SERVER_IP: &'static str = env!("OTA_IP");

const RX_BUFFER_SIZE: usize = 16384;
const TX_BUFFER_SIZE: usize = 16384;
static mut TX_BUFF: [u8; TX_BUFFER_SIZE] = [0; TX_BUFFER_SIZE];
static mut RX_BUFF: [u8; RX_BUFFER_SIZE] = [0; RX_BUFFER_SIZE];

#[main]
async fn main(spawner: Spawner) {
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::max(system.clock_control).freeze();
    let clocks = &*make_static!(clocks);
    //let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);

    esp_println::logger::init_logger_from_env();
    log::set_max_level(log::LevelFilter::Info);

    let mut rng = esp_hal::rng::Rng::new(peripherals.RNG);
    let timer = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG1, &clocks, None);
    let init = esp_wifi::initialize(
        esp_wifi::EspWifiInitFor::Wifi,
        timer.timer0,
        rng.clone(),
        peripherals.RADIO_CLK,
        &clocks,
    )
    .unwrap();

    let timg0 = TimerGroup::new_async(peripherals.TIMG0, &clocks);
    esp_hal_embassy::init(&clocks, timg0);

    let wifi = peripherals.WIFI;
    let (wifi_interface, controller) =
        esp_wifi::wifi::new_with_mode(&init, wifi, WifiStaDevice).unwrap();

    let config = Config::dhcpv4(Default::default());
    let seed = 69420;

    let stack = &*make_static!(Stack::new(
        wifi_interface,
        config,
        make_static!(StackResources::<3>::new()),
        seed,
    ));

    spawner
        .spawn(connection(controller, stack))
        .expect("connection spawn");
    spawner.spawn(net_task(stack)).expect("net task spawn");

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

    let mut ota = Ota::new(FlashStorage::new()).with_next_partition_offset();
    ota.set_flash_size(535008);

    let mut ota_buff = [0; 1024];
    loop {
        let res = socket.read(&mut ota_buff).await;
        if let Ok(n) = res {
            if n == 0 {
                break;
            }

            let start = Instant::now();
            let res = ota.ota_write_chunk(&ota_buff[..n]);
            log::info!("ota_write_res: {:?}", res);
            log::info!("Elapsed: {}", start.elapsed().as_micros());

            _ = socket.write(&[0]).await;
            if res == Ok(true) {
                _ = ota.ota_flush();
                break;
            }
        }

        Timer::after_millis(10).await;
    }

    loop {
        log::info!("bump");
        Timer::after_millis(1000).await;
    }
}

#[embassy_executor::task]
async fn connection(
    mut controller: WifiController<'static>,
    stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>,
) {
    log::info!("start connection task");
    log::info!("Device capabilities: {:?}", controller.get_capabilities());
    loop {
        if esp_wifi::wifi::get_wifi_state() == WifiState::StaConnected {
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
            controller.start().await.unwrap();
            log::info!("Wifi started!");
        }
        log::info!("About to connect...");

        match controller.connect().await {
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
async fn net_task(stack: &'static Stack<WifiDevice<'static, WifiStaDevice>>) {
    stack.run().await
}