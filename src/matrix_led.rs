//! matrix_ledの制御
//!  ledサイズ　32*8

use stm32f4::stm32f401;

/// Matrix Ledの制御
pub struct Matrix<'a> {
    video_ram: [u32; 8], // 左上を基点(0,0)として、各u32のMSBと[0]が基点
    device: &'a stm32f401::Peripherals,
    spi: &'a stm32f401::SPI1,
}

/// DMA転送用バッファ領域
static mut DMABUFF: [u16; 4] = [0u16; 4];

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
        for x in 0..8 {
            self.send_oneline_mat_led(x);
        }
    }

    /// Matrix LED に一行を送る
    /// # 引数
    ///     line_num:   一番上が0。一番下が7
    pub fn send_oneline_mat_led(&self, line_num: u32) {
        let digi_code: u16 = ((line_num + 1) << 8) as u16;
        let pat = self.video_ram[line_num as usize];
        let dat: [u16; 4] = [
            digi_code | (((pat >> 24) & 0x00FF) as u16),
            digi_code | (((pat >> 16) & 0x00FF) as u16),
            digi_code | (((pat >> 08) & 0x00FF) as u16),
            digi_code | (((pat) & 0x00FF) as u16),
        ];
        self.send_request_to_dma(&dat);
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

        for pat in &INIT_PAT {
            self.send_request_to_dma(&[*pat; 4]);
        }
    }

    /// SPI1 [u16;4]のデータのDMA送信要求
    ///   MatrixLED 4ブロック分のデータの送信を行う。
    ///   要求データが4ハーフワード(8バイト)より大きい場合は
    ///   末尾を末尾を切り捨てる。
    fn send_request_to_dma(&self, datas: &[u16]) {
        let dma = &self.device.DMA2;
        while dma.st[3].cr.read().en().is_enabled() {}
        unsafe {
            for i in 0..4 {
                DMABUFF[i] = datas[i]; //.swap_bytes();
            }
            let adr = DMABUFF.as_ptr() as u32;
            dma.st[3].m0ar.write(|w| w.m0a().bits(adr));
        }
        dma.st[3].ndtr.write(|w| w.ndt().bits(4u16));

        self.spi_enable();
        self.dma_start();
        while dma.lisr.read().tcif3().is_not_complete() {}
        dma.st[3].cr.modify(|_, w| w.en().disabled());
        while self.spi.sr.read().bsy().is_busy() {}
        self.spi_disable();
    }

    /// DMAの完了フラグをクリアし、DMAを開始する
    fn dma_start(&self) {
        let dma = &self.device.DMA2;
        dma.lifcr.write(|w| {
            w.ctcif3()
                .clear()
                .chtif3()
                .clear()
                .cteif3()
                .clear()
                .cdmeif3()
                .clear()
                .cfeif3()
                .clear()
        });
        dma.st[3].cr.modify(|_, w| w.en().enabled());
    }

    /// spi通信有効にセット
    fn spi_enable(&self) {
        self.cs_enable();
        self.spi.cr1.modify(|_, w| w.spe().enabled());
    }

    /// spi通信無効にセット
    ///   LEDのデータ確定シーケンス含む
    fn spi_disable(&self) {
        while self.spi.sr.read().txe().is_not_empty() {
            cortex_m::asm::nop();
        }
        while self.spi.sr.read().bsy().is_busy() {
            cortex_m::asm::nop(); // wait
        }
        self.cs_disable();
        self.spi.cr1.modify(|_, w| w.spe().disabled());
    }

    /// CS(DATA) ピンを 通信無効(HI)にする
    /// CSピンは、PA4に固定(ハードコート)
    fn cs_disable(&self) {
        self.device.GPIOA.bsrr.write(|w| w.bs4().set());
        for _x in 0..10 {
            // 通信終了時は、データの確定待ちが必要
            // 最低50ns 48MHzクロックで最低3クロック
            cortex_m::asm::nop();
        }
    }

    /// CS(DATA) ピンを通信有効(LO)にする
    /// CSピンは、PA4に固定(ハードコート)
    fn cs_enable(&self) {
        self.device.GPIOA.bsrr.write(|w| w.br4().reset());
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
        unsafe {
            st3_3.cr.modify(|_, w| w.chsel().bits(3u8));
        }
        st3_3.cr.modify(|_, w| {
            w.mburst()
                .incr4()
                .pburst()
                .single()
                .ct()
                .memory0()
                .dbm()
                .disabled()
                .pl()
                .medium()
                .pincos()
                .psize()
                .msize()
                .bits16()
                .psize()
                .bits16()
                .minc()
                .incremented()
                .pinc()
                .fixed()
                .circ()
                .disabled()
                .dir()
                .memory_to_peripheral()
                .tcie()
                .enabled()
                .htie()
                .disabled()
                .teie()
                .enabled()
                .dmeie()
                .enabled()
        });
        st3_3
            .fcr
            .modify(|_, w| w.feie().enabled().dmdis().disabled().fth().half());
        let spi1_dr = &self.device.SPI1.dr as *const _ as u32;
        st3_3.par.write(|w| w.pa().bits(spi1_dr));
    }
}
