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

use misakifont::font88::FONT88;

#[entry]
fn main() -> ! {
    let device = stm32f401::Peripherals::take().unwrap();

    init_clock(&device);
    gpio_setup(&device);
    spi1_setup(&device);
    init_mat_led(&device);

    //device.GPIOA.bsrr.write(|w| w.bs1().set());
    device.GPIOA.bsrr.write(|w| w.bs10().set());
    //device.GPIOA.bsrr.write(|w| w.bs11().set());

    let mut video = [0u32; 8];
    //let font = FONT48.get_char(0x52);
    let chars=[0xc3,0xdd,0xb2,0xd6,0xcd,0xa5,0xbb,0xd2];
    let mut x = 24;
    for c in 0..4 {
        let font = FONT88.get_char(chars[c*2],chars[c*2+1]);
        for i in 0..8 {
            video[i] |= ( font[i] as u32) << x ;
        }
        x -= 8;
    }
    for i in 0..8 {
        send_oneline_mat_led(&device, i, video[i as usize]);
    }



    device.GPIOA.bsrr.write(|w| w.bs11().set());

    loop {
        // your code goes here
    }
}

/// Matrix LED に一行を送る
/// # 引数
///     line_num:   一番上が0。一番下が7
///     pat:        パターン。一番左が最上位ビット
fn send_oneline_mat_led(device: &stm32f401::Peripherals, line_num: u32, pat: u32) {
    let digi_code :u16 = ((line_num+1)<<8) as u16;
    let dat :[u16; 4] = [   digi_code | (((pat>>24)&0x00FF) as u16),
                            digi_code | (((pat>>16)&0x00FF) as u16),
                            digi_code | (((pat>>08)&0x00FF) as u16),
                            digi_code | (((pat)&0x00FF) as u16),
                        ];
    spi_enable(&device);
    device.GPIOA.bsrr.write(|w| w.br4().reset());
    for d in &dat {
        spi_send_word(&device, *d);
    }
    mat_led_data_enter(&device);
    spi_disable(&device);
}
                
/// Matrix LED 初期化
fn init_mat_led(device: &stm32f401::Peripherals) {
    const INIT_PAT: [u16; 5] = [0x0F00,  // テストモード解除
                                0x0900,  // BCDデコードバイパス 
                                0x0A03,  // 輝度制御　下位4bit MAX:F
                                0x0B07,  // スキャン桁指定 下位4bit MAX:7
                                0x0C01,  // シャットダウンモード　解除
                               ];
    device.GPIOA.bsrr.write(|w| w.bs1().set());

    for pat in &INIT_PAT {
        spi_enable(&device);
        device.GPIOA.bsrr.write(|w| w.br4().reset());
        for _x in 0..5 {
            spi_send_word(&device, *pat);
        }
        mat_led_data_enter(&device);
        spi_disable(&device);
    }
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
fn mat_led_data_enter(device : &stm32f401::Peripherals) {
    while device.SPI1.sr.read().txe().is_not_empty() {
        cortex_m::asm::nop();
    }
    while device.SPI1.sr.read().bsy().is_busy() { 
        cortex_m::asm::nop(); // wait
    } 
    device.GPIOA.bsrr.write(|w| w.bs4().set());
    for _x in 0..100 { // 最低50nsウェイト　48MHzクロックで3クロック
        cortex_m::asm::nop();
    }
    
    device.GPIOA.bsrr.write(|w| w.br1().reset());
}

/// spi通信有効にセット
fn spi_enable(device : &stm32f401::Peripherals) {
    device.GPIOA.bsrr.write(|w| w.br4().reset());
    device.SPI1.cr1.modify(|_,w| w.spe().enabled());
}

/// spi通信無効にセット
fn spi_disable(device: &stm32f401::Peripherals) {
    while device.SPI1.sr.read().txe().is_not_empty() {
        cortex_m::asm::nop();
    }
    while device.SPI1.sr.read().bsy().is_busy() { 
        cortex_m::asm::nop(); // wait
    } 
    device.SPI1.cr1.modify(|_,w| w.spe().disabled()); 
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

    // APB2のクロックを1/16
    //device.RCC.cfgr.modify(|_,w| w.ppre2().div2());
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
    gpioa.ospeedr.modify(|_,w| w.ospeedr7().very_high_speed());
    gpioa.otyper.modify(|_,w| w.ot7().push_pull()); 
    gpioa.moder.modify(|_,w| w.moder5().alternate()); // SPI1_CLK
    gpioa.afrl.modify(|_,w| w.afrl5().af5());
    gpioa.ospeedr.modify(|_,w| w.ospeedr5().very_high_speed());
    gpioa.otyper.modify(|_,w| w.ot5().push_pull());
    gpioa.moder.modify(|_,w| w.moder4().output());   // NSS(CS)
    gpioa.ospeedr.modify(|_,w| w.ospeedr4().very_high_speed());
    gpioa.otyper.modify(|_,w| w.ot4().push_pull());
}

/// SPIのセットアップ
fn spi1_setup(device : &stm32f401::Peripherals) {
    // 電源投入
    device.RCC.apb2enr.modify(|_,w| w.spi1en().enabled());

    let spi1 = &device.SPI1;
    spi1.cr1.modify(|_,w| w.bidimode().unidirectional());
    spi1.cr1.modify(|_,w| w.dff().sixteen_bit());
    spi1.cr1.modify(|_,w| w.lsbfirst().msbfirst());
    spi1.cr1.modify(|_,w| w.br().div4()); // 基準クロックは48MHz
    spi1.cr1.modify(|_,w| w.mstr().master());
//    spi1.cr1.modify(|_,w| w.cpol().idle_high());
    spi1.cr1.modify(|_,w| w.cpol().idle_low());
    spi1.cr1.modify(|_,w| w.cpha().first_edge());
//    spi1.cr1.modify(|_,w| w.cpha().second_edge());
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
