//! matrix_ledの制御
//!  ledサイズ　32*8

use stm32f4::stm32f401;
use stm32f4::stm32f401::interrupt;

/// Matrix Ledの制御
pub struct Matrix<'a> {
    video_ram: [u32; 8], // 左上を基点(0,0)として、各u32のMSBと[0]が基点
    device: &'a stm32f401::Peripherals,
    spi: &'a stm32f401::SPI1,
}

impl<'a> Matrix<'a> {
    pub fn new(device: &stm32f401::Peripherals) -> Matrix {
        let led = Matrix {
            video_ram: [0; 8],
            device,
            spi: &device.SPI1,
        };
        led.gpio_setup();
        led.spi1_setup();
        led.dma_setup();
        led.init_mat_led();
        led
    }

    /// Video RAMをクリアする
    pub fn clear(&mut self) {
        for line in &mut self.video_ram {
            *line = 0;
        }
    }

    /// 指定の場所に、指定の矩形のビットマップを表示する。
    ///
    /// 原点は、左上隅(0,0)。
    /// ビットマップの最大サイズは8*8。
    ///
    /// 幅が8未満の場合は、LSBより詰めること。
    /// 矩形の高さは、bitmapの要素数に等しい。
    pub fn draw_bitmap(&mut self, px: i32, py: u32, width: u32, bitmap: &[u8]) {
        let width = if width <= 8 { width as i32 } else { 8 };
        let shift: i32 = 31 - px - width + 1;
        let mask: u32 = (1 << width) - 1;
        let mut y = if py >= 8 { return } else { py as usize };
        for line in bitmap {
            self.video_ram[y] |= if shift >= 0 {
                ((*line as u32) & mask) << shift
            } else {
                ((*line as u32) & mask) >> -shift
            };
            y += 1;
            if y >= 8 {
                break;
            }
        }
    }

    /// Matrix LEDにvideo_ramの内容を表示する。
    pub fn flash_led(&self) {
        while let Err(_) = DMA_BUFF.clear_buff(self.device) {}
        for x in 0..8 {
            self.send_oneline_mat_led(x);
        }
        self.send_request_to_dma();
    }

    /// Matrix LED BUFFに一行を送る
    /// # 引数
    ///     line_num:   一番上が0。一番下が7
    fn send_oneline_mat_led(&self, line_num: u32) {
        let digi_code: u16 = ((line_num + 1) << 8) as u16;
        let pat = self.video_ram[line_num as usize];
        let dat: [u16; 4] = [
            digi_code | (((pat >> 24) & 0x00FF) as u16),
            digi_code | (((pat >> 16) & 0x00FF) as u16),
            digi_code | (((pat >> 08) & 0x00FF) as u16),
            digi_code | (((pat) & 0x00FF) as u16),
        ];
        DMA_BUFF.add_buff(&dat, self.device).unwrap();
    }

    /// Matrix LED 初期化
    fn init_mat_led(&self) {
        const INIT_PAT: [u16; 5] = [
            0x0F00, // テストモード解除
            0x0900, // BCDデコードバイパス
            0x0A02, // 輝度制御　下位4bit MAX:F
            0x0B07, // スキャン桁指定 下位4bit MAX:7
            0x0C01, // シャットダウンモード　解除
        ];

        while let Err(_) = DMA_BUFF.clear_buff(self.device) {}
        for pat in &INIT_PAT {
            DMA_BUFF.add_buff(&[*pat; 4], self.device).unwrap();
        }
        self.send_request_to_dma();
    }

    /// SPI1 データのDMA送信要求
    ///   MatrixLED 4ブロック*行数 分のデータの送信を行う。
    ///   送信データは、事前にDMA_BUFFに投入済みのこと。
    fn send_request_to_dma(&self) {
        let dma = &self.device.DMA2;
        let mut i = DMA_BUFF.iter();
        if let Some(data) = i.next() {
            while dma.st[3].cr.read().en().is_enabled() {}
            let adr = data.as_ptr() as u32;
            dma.st[3].m0ar.write(|w| w.m0a().bits(adr));
            dma.st[3].ndtr.write(|w| w.ndt().bits(4u16));

            Self::spi_enable(&self.device);
            Self::dma_start(&self.device);
        }
        // 以降、2レコード目からの転送は、割込みルーチンにて
    }

    /// SPI送信終了待ちと送信終了時間の計測
    /// 　ループ回数が一定回数以上になると、緑のLEDを点灯する
    fn wait_api_and_measurement(device: &stm32f401::Peripherals) {
        let dma = &device.DMA2;
        let spi = &device.SPI1;
        let gpioa = &device.GPIOA;
        const WAIT_LIMIT: u32 = 31;
        let mut count_wait = 0;

        while dma.lisr.read().tcif3().is_not_complete() {
            count_wait += 1;
        }
        while spi.sr.read().txe().is_not_empty() {
            count_wait += 0;
        }
        while spi.sr.read().bsy().is_busy() {
            count_wait += 0;
        }
        if count_wait > WAIT_LIMIT {
            gpioa.bsrr.write(|w| w.bs0().set());
        }
    }

    /// DMAの完了フラグをクリアし、DMAを開始する
    fn dma_start(device: &stm32f401::Peripherals) {
        let dma = &device.DMA2;
        dma.lifcr.write(|w| {
            w.ctcif3().clear();
            w.chtif3().clear();
            w.cteif3().clear();
            w.cdmeif3().clear()
        });

        dma.st[3].cr.modify(|_, w| w.en().enabled());
    }

    /// spi通信有効にセット
    fn spi_enable(device: &stm32f401::Peripherals) {
        let spi = &device.SPI1;
        Self::cs_enable(&device);
        spi.cr1.modify(|_, w| w.spe().enabled());
    }

    /// spi通信無効にセット
    ///   LEDのデータ確定シーケンス含む
    fn spi_disable(device: &stm32f401::Peripherals) {
        let spi = &device.SPI1;
        while spi.sr.read().txe().is_not_empty() {
            cortex_m::asm::nop();
        }
        while spi.sr.read().bsy().is_busy() {
            cortex_m::asm::nop(); // wait
        }
        Self::cs_disable(&device);
        spi.cr1.modify(|_, w| w.spe().disabled());
    }

    /// CS(DATA) ピンを 通信無効(HI)にする
    /// CSピンは、PA4に固定(ハードコート)
    fn cs_disable(device: &stm32f401::Peripherals) {
        device.GPIOA.bsrr.write(|w| w.bs4().set());
        for _x in 0..5 {
            // 通信終了時は、データの確定待ちが必要
            // 最低50ns 48MHzクロックで最低3クロック
            cortex_m::asm::nop();
        }
    }

    /// CS(DATA) ピンを通信有効(LO)にする
    /// CSピンは、PA4に固定(ハードコート)
    fn cs_enable(device: &stm32f401::Peripherals) {
        device.GPIOA.bsrr.write(|w| w.br4().reset());
    }

    /// SPIのセットアップ
    fn spi1_setup(&self) {
        // 電源投入
        self.device.RCC.apb2enr.modify(|_, w| w.spi1en().enabled());

        self.spi.cr1.modify(|_, w| {
            w.bidimode().unidirectional().
            dff().sixteen_bit().
            lsbfirst().msbfirst().
            br().div4(). // 基準クロックは48MHz
            mstr().master().
            cpol().idle_low().
            cpha().first_edge().
            ssm().enabled().
            ssi().slave_not_selected()
        });
        self.spi.cr2.modify(|_, w| w.txdmaen().enabled());
    }

    /// gpioのセットアップ
    fn gpio_setup(&self) {
        self.device.RCC.ahb1enr.modify(|_, w| w.gpioaen().enabled());
        // SPI端子割付け
        let gpioa = &self.device.GPIOA;
        gpioa.moder.modify(|_, w| w.moder7().alternate()); // SPI1_MOSI
        gpioa.afrl.modify(|_, w| w.afrl7().af5());
        gpioa.ospeedr.modify(|_, w| w.ospeedr7().very_high_speed());
        gpioa.otyper.modify(|_, w| w.ot7().push_pull());
        gpioa.moder.modify(|_, w| w.moder5().alternate()); // SPI1_CLK
        gpioa.afrl.modify(|_, w| w.afrl5().af5());
        gpioa.ospeedr.modify(|_, w| w.ospeedr5().very_high_speed());
        gpioa.otyper.modify(|_, w| w.ot5().push_pull());
        gpioa.moder.modify(|_, w| w.moder4().output()); // NSS(CS)
        gpioa.ospeedr.modify(|_, w| w.ospeedr4().very_high_speed());
        gpioa.otyper.modify(|_, w| w.ot4().push_pull());
    }

    /// DMAのセットアップ
    fn dma_setup(&self) {
        self.device.RCC.ahb1enr.modify(|_, w| w.dma2en().enabled());
        // DMAストリーム3のチャンネル3使用
        let st3_3 = &self.device.DMA2.st[3];
        st3_3.cr.modify(|_, w| {
            w.chsel().bits(3u8);
            w.mburst().incr4();
            w.pburst().single();
            w.ct().memory0();
            w.dbm().disabled();
            w.pl().medium();
            w.pincos().psize();
            w.msize().bits16();
            w.psize().bits16();
            w.minc().incremented();
            w.pinc().fixed();
            w.circ().disabled();
            w.dir().memory_to_peripheral();
            w.tcie().enabled();
            w.htie().disabled();
            w.teie().disabled();
            w.dmeie().disabled()
        });
        st3_3.fcr.modify(|_, w| {
            w.feie().disabled();
            w.dmdis().disabled();
            w.fth().half()
        });
        let spi1_dr = &self.device.SPI1.dr as *const _ as u32;
        st3_3.par.write(|w| w.pa().bits(spi1_dr));
        unsafe {
            cortex_m::peripheral::NVIC::unmask(stm32f401::interrupt::DMA2_STREAM3);
        }
    }
}

/// DMA2 Stream3 割込み関数
#[interrupt]
fn DMA2_STREAM3() {
    static mut ITER: DmaBuffIter = DmaBuffIter { cur_index: None };

    let device;
    unsafe {
        device = stm32f401::Peripherals::steal();
    }
    let dma = &device.DMA2;
    if dma.lisr.read().tcif3().is_complete() {
        dma.lifcr.write(|w| w.ctcif3().clear());
        if let None = ITER.cur_index {
            ITER.cur_index = Some(0);
        }
        match ITER.next() {
            Some(data) => {
                //次のデータの準備
                let adr = data.as_ptr() as u32;
                dma.st[3].m0ar.write(|w| w.m0a().bits(adr));
                dma.st[3].ndtr.write(|w| w.ndt().bits(4u16));

                //前データの確定終了処理
                Matrix::spi_disable(&device);

                //次のデータの送信開始
                Matrix::spi_enable(&device);
                Matrix::dma_start(&device);
            }
            None => {
                //前データの確定終了処理
                Matrix::spi_disable(&device);
                *ITER = DmaBuffIter { cur_index: None };
            }
        }
    } else {
        dma.lifcr.write(|w| {
            w.ctcif3().clear();
            w.chtif3().clear();
            w.cteif3().clear();
            w.cdmeif3().clear()
        });
    }
}

/// DMAバッファ領域
/// 　グローバル変数・matrix_ledモジュール以外での操作禁止
///   DMA2_S3CR.ENビットが0の時のみ操作可能
static DMA_BUFF: DmaBuff = DMA_BUFF_INIT;

type Result<T> = core::result::Result<T, &'static str>;

use core::cell::UnsafeCell;
struct DmaBuff {
    buff: UnsafeCell<[[u16; 4]; 8]>,
    data_count: UnsafeCell<usize>,
}

const DMA_BUFF_INIT: DmaBuff = DmaBuff {
    buff: UnsafeCell::new([[0u16; 4]; 8]),
    data_count: UnsafeCell::new(0),
};

unsafe impl Sync for DmaBuff {}

impl DmaBuff {
    pub fn clear_buff(&self, device: &stm32f401::Peripherals) -> Result<()> {
        Self::is_dma_inactive(device)?;
        unsafe {
            *self.data_count.get() = 0;
        }
        Ok(())
    }

    pub fn add_buff(&self, data: &[u16], device: &stm32f401::Peripherals) -> Result<()> {
        Self::is_dma_inactive(device)?;
        unsafe {
            if *self.data_count.get() < 8 {
                *self.data_count.get() += 1;
            } else {
                return Err("Buffer over flow");
            }
            &(*self.buff.get())[*self.data_count.get() - 1].clone_from_slice(&data[0..4]);
        }
        Ok(())
    }

    pub fn iter(&self) -> DmaBuffIter {
        DmaBuffIter { cur_index: None }
    }

    fn is_dma_inactive(device: &stm32f401::Peripherals) -> Result<()> {
        if device.DMA2.st[3].cr.read().en().is_enabled() {
            Err("DMA2 stream active")
        } else {
            Ok(())
        }
    }

    fn get_buff(&self, index: usize) -> Option<&[u16; 4]> {
        unsafe {
            if index < *self.data_count.get() {
                Some(&(*self.buff.get())[index])
            } else {
                None
            }
        }
    }
}

/// DmaBuff用Iterator
struct DmaBuffIter {
    cur_index: Option<usize>,
}

impl Iterator for DmaBuffIter {
    type Item = &'static [u16; 4];

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.cur_index {
            Some(i) => {
                *i += 1;
            }
            None => {
                self.cur_index = Some(0);
            }
        };
        DMA_BUFF.get_buff(self.cur_index.unwrap())
    }
}
