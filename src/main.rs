#![no_std]
#![no_main]

// pick a panicking behavior
extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// extern crate panic_abort; // requires nightly
// extern crate panic_itm; // logs messages over ITM; requires ITM support
// extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger

//use cortex_m::asm;
use cortex_m_rt::entry;
use stm32f4::stm32f401;
//use cortex_m_semihosting::dbg;

#[entry]
fn main() -> ! {
    let device = stm32f401::Peripherals::take().unwrap();

    init_clock(&device);
    gpio_setup(&device);
    spi1_setup(&device);

    device.GPIOA.bsrr.write(|w| w.bs1().set());
    // Matrix LED 初期設定
    spi_enable(&device);
    let init_pat = [0x0F00u16,  // テストモード解除
                    0x0900u16,  // BCDデコードバイパス 
                    0x0A0Au16,  // 輝度制御　下位4bit MAX:F
                    0x0B07u16,  // スキャン桁指定 下位4bit MAX:7
                    0x0C01u16,  // シャットダウンモード　解除
                    0x0101u16,  // 桁0に00001111
                    0x0202u16,  // 桁1に00001111
                    0x0303u16,  // 桁2に00001111
                    0x0404u16,  // 桁3に00001111
                    0x0505u16,  // 桁4に00001111
                    0x0606u16,  // 桁5に00001111
                    0x0707u16,  // 桁6に00001111
                    0x080Fu16,  // 桁7に00001111
    ];
    device.GPIOA.bsrr.write(|w| w.bs10().set());
    for pat in &init_pat {
        //dbg!(*pat);
        spi_send_word(&device, 0x0C00u16);
        spi_send_word(&device, 0x0C00u16);
        spi_send_word(&device, 0x0C00u16);
        spi_send_word(&device, *pat);
        led_matrix_data_enter(&device);
    }

    device.GPIOA.bsrr.write(|w| w.bs11().set());

    loop {
        // your code goes here
    }
}

/// spi通信有効にセット
fn spi_enable(device : &stm32f401::Peripherals) {
    device.SPI1.cr1.modify(|_,w| w.spe().enabled());
}

/// SPI1 16ビットのデータを送信する。
fn spi_send_word(device: &stm32f401::Peripherals, data: u16) {
    let spi1 = &device.SPI1;
    while spi1.sr.read().txe().is_not_empty() { 
        cortex_m::asm::nop(); // wait
    }
    spi1.dr.write(|w| w.dr().bits(data));
}

/// LEDへのデータ通信を確定する
fn led_matrix_data_enter(device : &stm32f401::Peripherals) {
    while device.SPI1.sr.read().txe().is_not_empty() {
        cortex_m::asm::nop();
    }
    while device.SPI1.sr.read().bsy().is_busy() { 
        cortex_m::asm::nop(); // wait
    } 
    device.GPIOA.bsrr.write(|w| w.bs4().set());
    for _x in 0..1_000 { // 最低50nsウェイト　48MHzクロックで3クロック
        cortex_m::asm::nop();
    }
    device.GPIOA.bsrr.write(|w| w.br4().reset());
}


/// システムクロックの初期設定
/// 　クロック周波数　48MHz
fn init_clock(device : &stm32f401::Peripherals) {

    // システムクロック　48MHz
    // PLLCFGR設定
    // hsi(16M)/8*192/8=48MHz
    {
        let pllcfgr = &device.RCC.pllcfgr;
        pllcfgr.modify(|_,w| w.pllsrc().hsi());
        pllcfgr.modify(|_,w| w.pllp().div8());
        pllcfgr.modify(|_,w| unsafe { w.plln().bits(192u16) });
        pllcfgr.modify(|_,w| unsafe { w.pllm().bits(8u8) });
    }

    // PLL起動
    device.RCC.cr.modify(|_,w| w.pllon().on());
    while device.RCC.cr.read().pllrdy().is_not_ready() {
        // PLLの安定をただひたすら待つ
    }

    // フラッシュ読み出し遅延の変更
    device.FLASH.acr.modify(|_,w| unsafe {w.latency().bits(1u8)});
    // システムクロックをPLLに切り替え
    device.RCC.cfgr.modify(|_,w| w.sw().pll());
    while !device.RCC.cfgr.read().sws().is_pll() { /*wait*/ }
}

/// gpioのセットアップ
fn gpio_setup(device : &stm32f401::Peripherals) {
    // GPIOA 電源
    device.RCC.ahb1enr.modify(|_,w| w.gpioaen().enabled());

    // GPIOC セットアップ
    let gpioa = &device.GPIOA;
    gpioa.moder.modify(|_,w| w.moder1().output());
    gpioa.moder.modify(|_,w| w.moder10().output());
    gpioa.moder.modify(|_,w| w.moder11().output());

    // SPI端子割付け
    gpioa.moder.modify(|_,w| w.moder7().alternate()); // SPI1_MOSI
    gpioa.afrl.modify(|_,w| w.afrl7().af5());
    gpioa.otyper.modify(|_,w| w.ot7().open_drain()); 
    gpioa.moder.modify(|_,w| w.moder5().alternate()); // SPI1_CLK
    gpioa.afrl.modify(|_,w| w.afrl5().af5());
    gpioa.otyper.modify(|_,w| w.ot5().open_drain());
    gpioa.moder.modify(|_,w| w.moder4().output());   // NSS(CS)
    gpioa.otyper.modify(|_,w| w.ot4().open_drain());
}

/// SPIのセットアップ
fn spi1_setup(device : &stm32f401::Peripherals) {
    // 電源投入
    device.RCC.apb2enr.modify(|_,w| w.spi1en().enabled());

    let spi1 = &device.SPI1;
    spi1.cr1.modify(|_,w| w.bidimode().unidirectional());
    spi1.cr1.modify(|_,w| w.dff().sixteen_bit());
    spi1.cr1.modify(|_,w| w.lsbfirst().msbfirst());
    spi1.cr1.modify(|_,w| w.br().div256()); // 187kHz
    spi1.cr1.modify(|_,w| w.mstr().master());
    spi1.cr1.modify(|_,w| w.cpha().second_edge());
    spi1.cr1.modify(|_,w| w.ssm().enabled());
    spi1.cr1.modify(|_,w| w.ssi().slave_not_selected());
}

/*
/// TIM11のセットアップ
fn tim11_setup(device : &stm32f401::Peripherals) {
    // TIM11 電源
    device.RCC.apb2enr.modify(|_,w| w.tim11en().enabled());

    // TIM11 セットアップ
    let tim11 = &device.TIM11;
    tim11.psc.modify(|_,w| w.psc().bits(48_000u16 - 1)); // 1ms
    tim11.arr.modify(|_,w| unsafe { w.arr().bits(500u16) }); // 500ms
}
*/
