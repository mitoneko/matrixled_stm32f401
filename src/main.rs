#![no_std]
#![no_main]

// pick a panicking behavior
extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
// extern crate panic_abort; // requires nightly
// extern crate panic_itm; // logs messages over ITM; requires ITM support
// extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger

use cortex_m_rt::entry;
use cortex_m::interrupt::free;
use stm32f4::stm32f401;
use stm32f4::stm32f401::interrupt;

//use cortex_m_semihosting::dbg;

use misakifont::font88::FONT88;
use matrixled::matrix_led;

const START_TIME:u16 = 1500u16;
const CONTICUE_TIME:u16 = 200u16;

static WAKE_TIMER :WakeTimer = WAKE_TIMER_INIT;

#[entry]
fn main() -> ! {
    let device = stm32f401::Peripherals::take().unwrap();

    init_clock(&device);
    gpio_setup(&device);
    spi1_setup(&device);
    tim11_setup(&device);


    let mut matrix = matrix_led::Matrix::new(&device);

    device.GPIOA.bsrr.write(|w| w.bs10().set());
    //device.GPIOA.bsrr.write(|w| w.bs11().set());

    let chars=[
                0xa4,0xb3,0xa4,0xf3,0xa4,0xcb,0xa4,0xc1,0xa4,0xcf,0xa1,0xa2,
                0xc8,0xfe,0xc5,0xd4,0xa4,0xb5,0xa4,0xf3,
                0xa1,0xa1,0xa1,0xa1,0xa1,0xa1,0xa1,0xa1,
              ];

    device.GPIOA.bsrr.write(|w| w.bs11().set());

    let tim11 = &device.TIM11;
    tim11.arr.modify(|_,w| unsafe { w.arr().bits(START_TIME) }); 
    tim11.cr1.modify(|_,w| w.cen().enabled());
    free(|cs| WAKE_TIMER.set(cs));

    let char_count = chars.len()/2;
    let mut start_point = 0;
    loop {
        if free(|cs| WAKE_TIMER.get(cs)) { // タイマー割込みの確認
            if start_point==0 {
                tim11.arr.modify(|_,w| unsafe { w.arr().bits(START_TIME) }); 
            } else {
                tim11.arr.modify(|_,w| unsafe { w.arr().bits(CONTICUE_TIME) }); 
            }

            // 漢字の表示位置算出と描画
            matrix.clear();
            let char_start = start_point / 8;
            let char_end = if (start_point % 8)==0 {
                                    char_start+3
                                } else {
                                    char_start+4
                                };
            let char_end = core::cmp::min(char_end, char_count);
            let mut disp_xpos:i32 = -((start_point%8) as i32);
            for i in char_start..char_end+1 { // 各漢字の表示
                let font = FONT88.get_char(chars[i*2], chars[i*2+1]);
                matrix.draw_bitmap(disp_xpos, 0, 8, font);
                disp_xpos += 8;
            }
            matrix.flash_led(); // LED表示の更新
            start_point += 1;

            if start_point > 8*char_count - 32 {
                start_point = 0;
            }
            free(|cs| WAKE_TIMER.reset(cs));
        }

        device.GPIOA.bsrr.write(|w| w.br1().reset());
        cortex_m::asm::wfi();
        device.GPIOA.bsrr.write(|w| w.bs1().set());
    }
}

use core::cell::UnsafeCell;
/// TIM11割り込み関数
#[interrupt]
fn TIM1_TRG_COM_TIM11() {
    free(|cs| {
        unsafe {
            let device = stm32f401::Peripherals::steal();
            device.TIM11.sr.modify(|_,w| w.uif().clear());
        }
        WAKE_TIMER.set(cs);
    });
}

/// タイマーの起動を知らせるフラグ
/// グローバル　イミュータブル変数とする
struct WakeTimer(UnsafeCell<bool>);
const WAKE_TIMER_INIT: WakeTimer = WakeTimer(UnsafeCell::new(false));
impl WakeTimer {
    pub fn set(&self, _cs: &cortex_m::interrupt::CriticalSection) {
        unsafe { *self.0.get() = true };
    }
    pub fn reset(&self, _cs: &cortex_m::interrupt::CriticalSection) {
        unsafe { *self.0.get() = false };
    }
    pub fn get(&self, _cs: &cortex_m::interrupt::CriticalSection) -> bool {
        unsafe { *self.0.get() }
    }
}
unsafe impl Sync for WakeTimer { }

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
    spi1.cr1.modify(|_,w| w.cpol().idle_low());
    spi1.cr1.modify(|_,w| w.cpha().first_edge());
    spi1.cr1.modify(|_,w| w.ssm().enabled());
    spi1.cr1.modify(|_,w| w.ssi().slave_not_selected());
}

/// TIM11のセットアップ
fn tim11_setup(device : &stm32f401::Peripherals) {
    // TIM11 電源
    device.RCC.apb2enr.modify(|_,w| w.tim11en().enabled());

    // TIM11 セットアップ
    let tim11 = &device.TIM11;
    tim11.psc.modify(|_,w| w.psc().bits(48_000u16 - 1)); // 1ms
    tim11.dier.modify(|_,w| w.uie().enabled());
    unsafe {
        cortex_m::peripheral::NVIC::unmask(
            stm32f401::interrupt::TIM1_TRG_COM_TIM11);
    }
    
}
