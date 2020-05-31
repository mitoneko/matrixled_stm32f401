#![no_std]
#![no_main]

// pick a panicking behavior
extern crate panic_halt; // you can put a breakpoint on `rust_begin_unwind` to catch panics
                         // extern crate panic_abort; // requires nightly
                         // extern crate panic_itm; // logs messages over ITM; requires ITM support
                         // extern crate panic_semihosting; // logs messages to the host stderr; requires a debugger

use cortex_m::interrupt::free;
use cortex_m_rt::entry;
use stm32f4::stm32f401;
use stm32f4::stm32f401::interrupt;

//use cortex_m_semihosting::dbg;

use matrixled::display_led;
use matrixled::display_led::DisplayLed;
use matrixled::print_led;

const WAIT_TIME: u16 = 1000u16;

static WAKE_TIMER: WakeTimer = WAKE_TIMER_INIT;

#[entry]
fn main() -> ! {
    let device = stm32f401::Peripherals::take().unwrap();

    init_clock(&device);
    gpio_setup(&device);
    tim11_setup(&device);

    let mut led = DisplayLed::new(&device);

    //device.GPIOA.bsrr.write(|w| w.bs0().set());

    let tim11 = &device.TIM11;
    tim11.arr.modify(|_, w| unsafe { w.arr().bits(WAIT_TIME) });
    tim11.cr1.modify(|_, w| w.cen().enabled());
    free(|cs| WAKE_TIMER.set(cs));

    let mut count = 0;
    loop {
        // タイマー割込み確認
        if free(|cs| WAKE_TIMER.get(cs)) {
            count += 1;
            if count > 10000 {
                count = 0;
            }
            print_led!(led, "{}:{}\n", "EFG", count);

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
            device.TIM11.sr.modify(|_, w| w.uif().clear());
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
unsafe impl Sync for WakeTimer {}

/// システムクロックの初期設定
/// 　クロック周波数　48MHz
fn init_clock(device: &stm32f401::Peripherals) {
    // システムクロック　48MHz
    // PLLCFGR設定
    // hsi(16M)/8*192/8=48MHz
    {
        let pllcfgr = &device.RCC.pllcfgr;
        pllcfgr.modify(|_, w| w.pllsrc().hsi());
        pllcfgr.modify(|_, w| w.pllp().div8());
        pllcfgr.modify(|_, w| unsafe { w.plln().bits(192u16) });
        pllcfgr.modify(|_, w| unsafe { w.pllm().bits(8u8) });
    }

    // PLL起動
    device.RCC.cr.modify(|_, w| w.pllon().on());
    while device.RCC.cr.read().pllrdy().is_not_ready() {
        // PLLの安定をただひたすら待つ
    }

    // フラッシュ読み出し遅延の変更
    device
        .FLASH
        .acr
        .modify(|_, w| unsafe { w.latency().bits(1u8) });
    // システムクロックをPLLに切り替え
    device.RCC.cfgr.modify(|_, w| w.sw().pll());
    while !device.RCC.cfgr.read().sws().is_pll() { /*wait*/ }

    // APB2のクロックを1/16
    //device.RCC.cfgr.modify(|_,w| w.ppre2().div2());
}

/// gpioのセットアップ
fn gpio_setup(device: &stm32f401::Peripherals) {
    // GPIOA 電源
    device.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());

    // GPIOC セットアップ
    let gpioa = &device.GPIOA;
    gpioa.moder.modify(|_, w| w.moder1().output());
    gpioa.moder.modify(|_, w| w.moder0().output());
    gpioa.moder.modify(|_, w| w.moder11().output());
}

/// TIM11のセットアップ
fn tim11_setup(device: &stm32f401::Peripherals) {
    // TIM11 電源
    device.RCC.apb2enr.modify(|_, w| w.tim11en().enabled());

    // TIM11 セットアップ
    let tim11 = &device.TIM11;
    tim11.psc.modify(|_, w| w.psc().bits(48_000u16 - 1)); // 1ms
    tim11.dier.modify(|_, w| w.uie().enabled());
    unsafe {
        cortex_m::peripheral::NVIC::unmask(stm32f401::interrupt::TIM1_TRG_COM_TIM11);
    }
}
