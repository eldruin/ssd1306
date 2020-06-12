//! Container to store and set display properties

use crate::mode::displaymode::DisplayModeTrait;
use crate::{
    brightness::Brightness,
    command::{AddrMode, Command, VcomhLevel},
    displayrotation::DisplayRotation,
    displaysize::DisplaySize,
};
use display_interface::{DataFormat::U8, DisplayError, WriteOnlyDataCommand};

/// Display properties struct
pub struct DisplayProperties<DI> {
    iface: DI,
    display_size: DisplaySize,
    display_rotation: DisplayRotation,
    pub(crate) display_offset: (u8, u8),
    addr_mode: AddrMode,
}

impl<DI> DisplayProperties<DI> {
    /// Create new DisplayProperties instance
    pub fn new(
        iface: DI,
        display_size: DisplaySize,
        display_rotation: DisplayRotation,
    ) -> DisplayProperties<DI> {
        let display_offset = match display_size {
            DisplaySize::Display128x64 => (0, 0),
            DisplaySize::Display128x32 => (0, 0),
            DisplaySize::Display96x16 => (0, 0),
            DisplaySize::Display72x40 => (28, 0),
            DisplaySize::Display64x48 => (32, 0),
        };

        DisplayProperties {
            iface,
            display_size,
            display_rotation,
            display_offset,
            addr_mode: AddrMode::Page, // reset value
        }
    }

    /// Releases the display interface
    pub fn release(self) -> DI {
        self.iface
    }
}

impl<DI> DisplayProperties<DI>
where
    DI: WriteOnlyDataCommand,
{
    /// Initialise the display in column mode (i.e. a byte walks down a column of 8 pixels) with
    /// column 0 on the left and column _(display_width - 1)_ on the right.
    pub fn init_column_mode(&mut self) -> Result<(), DisplayError> {
        self.init_with_mode(AddrMode::Horizontal)
    }

    /// Initialise the display in one of the available addressing modes
    pub fn init_with_mode(&mut self, mode: AddrMode) -> Result<(), DisplayError> {
        // TODO: Break up into nice bits so display modes can pick whathever they need
        let (_, display_height) = self.display_size.dimensions();

        let display_rotation = self.display_rotation;

        Command::DisplayOn(false).send(&mut self.iface)?;
        Command::DisplayClockDiv(0x8, 0x0).send(&mut self.iface)?;
        Command::Multiplex(display_height - 1).send(&mut self.iface)?;
        Command::DisplayOffset(0).send(&mut self.iface)?;
        Command::StartLine(0).send(&mut self.iface)?;
        // TODO: Ability to turn charge pump on/off
        Command::ChargePump(true).send(&mut self.iface)?;
        Command::AddressMode(mode).send(&mut self.iface)?;

        self.set_rotation(display_rotation)?;

        match self.display_size {
            DisplaySize::Display128x32 => Command::ComPinConfig(false, false).send(&mut self.iface),
            DisplaySize::Display128x64 => Command::ComPinConfig(true, false).send(&mut self.iface),
            DisplaySize::Display96x16 => Command::ComPinConfig(false, false).send(&mut self.iface),
            DisplaySize::Display72x40 => Command::ComPinConfig(true, false).send(&mut self.iface),
            DisplaySize::Display64x48 => Command::ComPinConfig(true, false).send(&mut self.iface),
        }?;

        self.change_brightness(Brightness::default())?;
        Command::VcomhDeselect(VcomhLevel::Auto).send(&mut self.iface)?;
        Command::AllOn(false).send(&mut self.iface)?;
        Command::Invert(false).send(&mut self.iface)?;
        Command::EnableScroll(false).send(&mut self.iface)?;
        Command::DisplayOn(true).send(&mut self.iface)?;

        self.addr_mode = mode;

        Ok(())
    }

    /// Change the addressing mode
    pub fn change_mode(&mut self, mode: AddrMode) -> Result<(), DisplayError> {
        Command::AddressMode(mode).send(&mut self.iface)?;
        self.addr_mode = mode;
        Ok(())
    }

    /// Set the position in the framebuffer of the display limiting where any sent data should be
    /// drawn. This method can be used for changing the affected area on the screen as well
    /// as (re-)setting the start point of the next `draw` call.
    /// Only works in Horizontal or Vertical addressing mode
    pub fn set_draw_area(&mut self, start: (u8, u8), end: (u8, u8)) -> Result<(), DisplayError> {
        match self.addr_mode {
            AddrMode::Page => panic!("Device cannot be in Page mode to set draw area"),
            _ => {
                Command::ColumnAddress(start.0, end.0 - 1).send(&mut self.iface)?;
                Command::PageAddress(start.1.into(), (end.1 - 1).into()).send(&mut self.iface)?;
                Ok(())
            }
        }
    }

    /// Set the column address in the framebuffer of the display where any sent data should be
    /// drawn.
    /// Only works in Page addressing mode.
    pub fn set_column(&mut self, column: u8) -> Result<(), DisplayError> {
        match self.addr_mode {
            AddrMode::Page => Command::ColStart(column).send(&mut self.iface),
            _ => panic!("Device must be in Page mode to set column"),
        }
    }

    /// Set the page address (row 8px high) in the framebuffer of the display where any sent data
    /// should be drawn.
    /// Note that the parameter is in pixels, but the page will be set to the start of the 8px
    /// row which contains the passed-in row.
    /// Only works in Page addressing mode.
    pub fn set_row(&mut self, row: u8) -> Result<(), DisplayError> {
        match self.addr_mode {
            AddrMode::Page => Command::PageStart(row.into()).send(&mut self.iface),
            _ => panic!("Device must be in Page mode to set row"),
        }
    }

    /// Send the data to the display for drawing at the current position in the framebuffer
    /// and advance the position accordingly. Cf. `set_draw_area` to modify the area affected by
    /// this method in horizontal / vertical mode.
    pub fn draw(&mut self, buffer: &[u8]) -> Result<(), DisplayError> {
        self.iface.send_data(U8(&buffer))
    }

    /// Send the data to the display for drawing at the current position in the framebuffer
    /// and advance the position accordingly. Cf. `set_draw_area` to modify the affected area by
    /// this method.
    ///
    /// This method takes advantage of a bounding box for faster writes.
    pub fn bounded_draw(
        &mut self,
        buffer: &[u8],
        disp_width: usize,
        upper_left: (u8, u8),
        lower_right: (u8, u8),
    ) -> Result<(), DisplayError> {
        // Divide by 8 since each row is actually 8 pixels tall
        let num_pages = ((lower_right.1 - upper_left.1) / 8) as usize + 1;

        // Each page is 8 bits tall, so calculate which page number to start at (rounded down) from
        // the top of the display
        let starting_page = (upper_left.1 / 8) as usize;

        // Calculate start and end X coordinates for each page
        let page_lower = upper_left.0 as usize;
        let page_upper = lower_right.0 as usize;

        buffer
            .chunks(disp_width)
            .skip(starting_page)
            .take(num_pages)
            .map(|s| &s[page_lower..page_upper])
            .try_for_each(|c| self.iface.send_data(U8(&c)))
    }

    /// Get the configured display size
    pub fn get_size(&self) -> DisplaySize {
        self.display_size
    }

    /// Get display dimensions, taking into account the current rotation of the display
    ///
    /// ```rust
    /// # use ssd1306::{properties::DisplayProperties, test_helpers::StubInterface};
    /// # let interface = StubInterface;
    /// use ssd1306::prelude::*;
    /// #
    /// let disp = DisplayProperties::new(
    ///     interface,
    ///     DisplaySize::Display128x64,
    ///     DisplayRotation::Rotate0,
    /// );
    /// assert_eq!(disp.get_dimensions(), (128, 64));
    ///
    /// # let interface = StubInterface;
    /// let rotated_disp = DisplayProperties::new(
    ///     interface,
    ///     DisplaySize::Display128x64,
    ///     DisplayRotation::Rotate90,
    /// );
    /// assert_eq!(rotated_disp.get_dimensions(), (64, 128));
    /// ```
    pub fn get_dimensions(&self) -> (u8, u8) {
        let (w, h) = self.display_size.dimensions();

        match self.display_rotation {
            DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => (w, h),
            DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => (h, w),
        }
    }

    /// Get the display rotation
    pub fn get_rotation(&self) -> DisplayRotation {
        self.display_rotation
    }

    /// Set the display rotation
    pub fn set_rotation(&mut self, display_rotation: DisplayRotation) -> Result<(), DisplayError> {
        self.display_rotation = display_rotation;

        match display_rotation {
            DisplayRotation::Rotate0 => {
                Command::SegmentRemap(true).send(&mut self.iface)?;
                Command::ReverseComDir(true).send(&mut self.iface)?;
            }
            DisplayRotation::Rotate90 => {
                Command::SegmentRemap(false).send(&mut self.iface)?;
                Command::ReverseComDir(true).send(&mut self.iface)?;
            }
            DisplayRotation::Rotate180 => {
                Command::SegmentRemap(false).send(&mut self.iface)?;
                Command::ReverseComDir(false).send(&mut self.iface)?;
            }
            DisplayRotation::Rotate270 => {
                Command::SegmentRemap(true).send(&mut self.iface)?;
                Command::ReverseComDir(false).send(&mut self.iface)?;
            }
        };

        Ok(())
    }

    /// Turn the display on or off. The display can be drawn to and retains all
    /// of its memory even while off.
    pub fn display_on(&mut self, on: bool) -> Result<(), DisplayError> {
        Command::DisplayOn(on).send(&mut self.iface)
    }

    /// Change the display brightness.
    pub fn change_brightness(&mut self, brightness: Brightness) -> Result<(), DisplayError> {
        // Should be moved to Brightness::new once conditions can be used in const functions
        debug_assert!(
            0 < brightness.precharge && brightness.precharge <= 15,
            "Precharge value must be between 1 and 15"
        );

        Command::PreChargePeriod(1, brightness.precharge).send(&mut self.iface)?;
        Command::Contrast(brightness.contrast).send(&mut self.iface)
    }

    /// Change into any mode implementing DisplayModeTrait
    pub fn into<NMODE: DisplayModeTrait<DI>>(self) -> NMODE
    where
        DI: WriteOnlyDataCommand,
    {
        NMODE::new(self)
    }
}
