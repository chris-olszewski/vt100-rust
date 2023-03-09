use crate::term::BufWrite as _;
use unicode_width::UnicodeWidthChar as _;

const MODE_APPLICATION_KEYPAD: u8 = 0b0000_0001;
const MODE_APPLICATION_CURSOR: u8 = 0b0000_0010;
const MODE_HIDE_CURSOR: u8 = 0b0000_0100;
const MODE_ALTERNATE_SCREEN: u8 = 0b0000_1000;
const MODE_BRACKETED_PASTE: u8 = 0b0001_0000;

/// The xterm mouse handling mode currently in use.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MouseProtocolMode {
    /// Mouse handling is disabled.
    None,

    /// Mouse button events should be reported on button press. Also known as
    /// X10 mouse mode.
    Press,

    /// Mouse button events should be reported on button press and release.
    /// Also known as VT200 mouse mode.
    PressRelease,

    // Highlight,
    /// Mouse button events should be reported on button press and release, as
    /// well as when the mouse moves between cells while a button is held
    /// down.
    ButtonMotion,

    /// Mouse button events should be reported on button press and release,
    /// and mouse motion events should be reported when the mouse moves
    /// between cells regardless of whether a button is held down or not.
    AnyMotion,
    // DecLocator,
}

impl Default for MouseProtocolMode {
    fn default() -> Self {
        Self::None
    }
}

/// The encoding to use for the enabled `MouseProtocolMode`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum MouseProtocolEncoding {
    /// Default single-printable-byte encoding.
    Default,

    /// UTF-8-based encoding.
    Utf8,

    /// SGR-like encoding.
    Sgr,
    // Urxvt,
}

impl Default for MouseProtocolEncoding {
    fn default() -> Self {
        Self::Default
    }
}

/// Represents the overall terminal state.
#[derive(Clone, Debug)]
pub struct Screen {
    grid: crate::grid::Grid,
    alternate_grid: crate::grid::Grid,

    attrs: crate::attrs::Attrs,
    saved_attrs: crate::attrs::Attrs,

    title: String,
    icon_name: String,

    modes: u8,
    mouse_protocol_mode: MouseProtocolMode,
    mouse_protocol_encoding: MouseProtocolEncoding,

    audible_bell_count: usize,
    visual_bell_count: usize,

    errors: usize,
}

impl Screen {
    pub(crate) fn new(
        size: crate::grid::Size,
        scrollback_len: usize,
    ) -> Self {
        let mut grid = crate::grid::Grid::new(size, scrollback_len);
        grid.allocate_rows();
        Self {
            grid,
            alternate_grid: crate::grid::Grid::new(size, 0),

            attrs: crate::attrs::Attrs::default(),
            saved_attrs: crate::attrs::Attrs::default(),

            title: String::default(),
            icon_name: String::default(),

            modes: 0,
            mouse_protocol_mode: MouseProtocolMode::default(),
            mouse_protocol_encoding: MouseProtocolEncoding::default(),

            audible_bell_count: 0,
            visual_bell_count: 0,

            errors: 0,
        }
    }

    /// Resizes the terminal.
    pub fn set_size(&mut self, rows: u16, cols: u16) {
        self.grid.set_size(crate::grid::Size { rows, cols });
        self.alternate_grid
            .set_size(crate::grid::Size { rows, cols });
    }

    /// Returns the current size of the terminal.
    ///
    /// The return value will be (rows, cols).
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        let size = self.grid().size();
        (size.rows, size.cols)
    }

    /// Scrolls to the given position in the scrollback.
    ///
    /// This position indicates the offset from the top of the screen, and
    /// should be `0` to put the normal screen in view.
    ///
    /// This affects the return values of methods called on the screen: for
    /// instance, `screen.cell(0, 0)` will return the top left corner of the
    /// screen after taking the scrollback offset into account.
    ///
    /// The value given will be clamped to the actual size of the scrollback.
    pub fn set_scrollback(&mut self, rows: usize) {
        self.grid_mut().set_scrollback(rows);
    }

    /// Returns the current position in the scrollback.
    ///
    /// This position indicates the offset from the top of the screen, and is
    /// `0` when the normal screen is in view.
    #[must_use]
    pub fn scrollback(&self) -> usize {
        self.grid().scrollback()
    }

    /// Returns the text contents of the terminal.
    ///
    /// This will not include any formatting information, and will be in plain
    /// text format.
    #[must_use]
    pub fn contents(&self) -> String {
        let mut contents = String::new();
        self.write_contents(&mut contents);
        contents
    }

    fn write_contents(&self, contents: &mut String) {
        self.grid().write_contents(contents);
    }

    /// Returns the text contents of the terminal by row, restricted to the
    /// given subset of columns.
    ///
    /// This will not include any formatting information, and will be in plain
    /// text format.
    ///
    /// Newlines will not be included.
    pub fn rows(
        &self,
        start: u16,
        width: u16,
    ) -> impl Iterator<Item = String> + '_ {
        self.grid().visible_rows().map(move |row| {
            let mut contents = String::new();
            row.write_contents(&mut contents, start, width, false);
            contents
        })
    }

    /// Returns the text contents of the terminal logically between two cells.
    /// This will include the remainder of the starting row after `start_col`,
    /// followed by the entire contents of the rows between `start_row` and
    /// `end_row`, followed by the beginning of the `end_row` up until
    /// `end_col`. This is useful for things like determining the contents of
    /// a clipboard selection.
    #[must_use]
    pub fn contents_between(
        &self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
    ) -> String {
        match start_row.cmp(&end_row) {
            std::cmp::Ordering::Less => {
                let (_, cols) = self.size();
                let mut contents = String::new();
                for (i, row) in self
                    .grid()
                    .visible_rows()
                    .enumerate()
                    .skip(usize::from(start_row))
                    .take(usize::from(end_row) - usize::from(start_row) + 1)
                {
                    if i == usize::from(start_row) {
                        row.write_contents(
                            &mut contents,
                            start_col,
                            cols - start_col,
                            false,
                        );
                        if !row.wrapped() {
                            contents.push('\n');
                        }
                    } else if i == usize::from(end_row) {
                        row.write_contents(&mut contents, 0, end_col, false);
                    } else {
                        row.write_contents(&mut contents, 0, cols, false);
                        if !row.wrapped() {
                            contents.push('\n');
                        }
                    }
                }
                contents
            }
            std::cmp::Ordering::Equal => {
                if start_col < end_col {
                    self.rows(start_col, end_col - start_col)
                        .nth(usize::from(start_row))
                        .unwrap_or_default()
                } else {
                    String::new()
                }
            }
            std::cmp::Ordering::Greater => String::new(),
        }
    }

    /// Return escape codes sufficient to reproduce the entire contents of the
    /// current terminal state. This is a convenience wrapper around
    /// `contents_formatted`, `input_mode_formatted`, and `title_formatted`.
    #[must_use]
    pub fn state_formatted(&self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_contents_formatted(&mut contents);
        self.write_input_mode_formatted(&mut contents);
        self.write_title_formatted(&mut contents);
        contents
    }

    /// Return escape codes sufficient to turn the terminal state of the
    /// screen `prev` into the current terminal state. This is a convenience
    /// wrapper around `contents_diff`, `input_mode_diff`, `title_diff`, and
    /// `bells_diff`.
    #[must_use]
    pub fn state_diff(&self, prev: &Self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_contents_diff(&mut contents, prev);
        self.write_input_mode_diff(&mut contents, prev);
        self.write_title_diff(&mut contents, prev);
        self.write_bells_diff(&mut contents, prev);
        contents
    }

    /// Returns the formatted visible contents of the terminal.
    ///
    /// Formatting information will be included inline as terminal escape
    /// codes. The result will be suitable for feeding directly to a raw
    /// terminal parser, and will result in the same visual output.
    #[must_use]
    pub fn contents_formatted(&self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_contents_formatted(&mut contents);
        contents
    }

    fn write_contents_formatted(&self, contents: &mut Vec<u8>) {
        crate::term::HideCursor::new(self.hide_cursor()).write_buf(contents);
        let prev_attrs = self.grid().write_contents_formatted(contents);
        self.attrs.write_escape_code_diff(contents, &prev_attrs);
    }

    /// Returns the formatted visible contents of the terminal by row,
    /// restricted to the given subset of columns.
    ///
    /// Formatting information will be included inline as terminal escape
    /// codes. The result will be suitable for feeding directly to a raw
    /// terminal parser, and will result in the same visual output.
    ///
    /// You are responsible for positioning the cursor before printing each
    /// row, and the final cursor position after displaying each row is
    /// unspecified.
    // the unwraps in this method shouldn't be reachable
    #[allow(clippy::missing_panics_doc)]
    pub fn rows_formatted(
        &self,
        start: u16,
        width: u16,
    ) -> impl Iterator<Item = Vec<u8>> + '_ {
        let mut wrapping = false;
        self.grid().visible_rows().enumerate().map(move |(i, row)| {
            // number of rows in a grid is stored in a u16 (see Size), so
            // visible_rows can never return enough rows to overflow here
            let i = i.try_into().unwrap();
            let mut contents = vec![];
            row.write_contents_formatted(
                &mut contents,
                start,
                width,
                i,
                wrapping,
                None,
                None,
            );
            if start == 0 && width == self.grid.size().cols {
                wrapping = row.wrapped();
            }
            contents
        })
    }

    /// Returns a terminal byte stream sufficient to turn the visible contents
    /// of the screen described by `prev` into the visible contents of the
    /// screen described by `self`.
    ///
    /// The result of rendering `prev.contents_formatted()` followed by
    /// `self.contents_diff(prev)` should be equivalent to the result of
    /// rendering `self.contents_formatted()`. This is primarily useful when
    /// you already have a terminal parser whose state is described by `prev`,
    /// since the diff will likely require less memory and cause less
    /// flickering than redrawing the entire screen contents.
    #[must_use]
    pub fn contents_diff(&self, prev: &Self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_contents_diff(&mut contents, prev);
        contents
    }

    fn write_contents_diff(&self, contents: &mut Vec<u8>, prev: &Self) {
        if self.hide_cursor() != prev.hide_cursor() {
            crate::term::HideCursor::new(self.hide_cursor())
                .write_buf(contents);
        }
        let prev_attrs = self.grid().write_contents_diff(
            contents,
            prev.grid(),
            prev.attrs,
        );
        self.attrs.write_escape_code_diff(contents, &prev_attrs);
    }

    /// Returns a sequence of terminal byte streams sufficient to turn the
    /// visible contents of the subset of each row from `prev` (as described
    /// by `start` and `width`) into the visible contents of the corresponding
    /// row subset in `self`.
    ///
    /// You are responsible for positioning the cursor before printing each
    /// row, and the final cursor position after displaying each row is
    /// unspecified.
    // the unwraps in this method shouldn't be reachable
    #[allow(clippy::missing_panics_doc)]
    pub fn rows_diff<'a>(
        &'a self,
        prev: &'a Self,
        start: u16,
        width: u16,
    ) -> impl Iterator<Item = Vec<u8>> + 'a {
        self.grid()
            .visible_rows()
            .zip(prev.grid().visible_rows())
            .enumerate()
            .map(move |(i, (row, prev_row))| {
                // number of rows in a grid is stored in a u16 (see Size), so
                // visible_rows can never return enough rows to overflow here
                let i = i.try_into().unwrap();
                let mut contents = vec![];
                row.write_contents_diff(
                    &mut contents,
                    prev_row,
                    start,
                    width,
                    i,
                    false,
                    false,
                    crate::grid::Pos { row: i, col: start },
                    crate::attrs::Attrs::default(),
                );
                contents
            })
    }

    /// Returns terminal escape sequences sufficient to set the current
    /// terminal's input modes.
    ///
    /// Supported modes are:
    /// * application keypad
    /// * application cursor
    /// * bracketed paste
    /// * xterm mouse support
    #[must_use]
    pub fn input_mode_formatted(&self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_input_mode_formatted(&mut contents);
        contents
    }

    fn write_input_mode_formatted(&self, contents: &mut Vec<u8>) {
        crate::term::ApplicationKeypad::new(
            self.mode(MODE_APPLICATION_KEYPAD),
        )
        .write_buf(contents);
        crate::term::ApplicationCursor::new(
            self.mode(MODE_APPLICATION_CURSOR),
        )
        .write_buf(contents);
        crate::term::BracketedPaste::new(self.mode(MODE_BRACKETED_PASTE))
            .write_buf(contents);
        crate::term::MouseProtocolMode::new(
            self.mouse_protocol_mode,
            MouseProtocolMode::None,
        )
        .write_buf(contents);
        crate::term::MouseProtocolEncoding::new(
            self.mouse_protocol_encoding,
            MouseProtocolEncoding::Default,
        )
        .write_buf(contents);
    }

    /// Returns terminal escape sequences sufficient to change the previous
    /// terminal's input modes to the input modes enabled in the current
    /// terminal.
    #[must_use]
    pub fn input_mode_diff(&self, prev: &Self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_input_mode_diff(&mut contents, prev);
        contents
    }

    fn write_input_mode_diff(&self, contents: &mut Vec<u8>, prev: &Self) {
        if self.mode(MODE_APPLICATION_KEYPAD)
            != prev.mode(MODE_APPLICATION_KEYPAD)
        {
            crate::term::ApplicationKeypad::new(
                self.mode(MODE_APPLICATION_KEYPAD),
            )
            .write_buf(contents);
        }
        if self.mode(MODE_APPLICATION_CURSOR)
            != prev.mode(MODE_APPLICATION_CURSOR)
        {
            crate::term::ApplicationCursor::new(
                self.mode(MODE_APPLICATION_CURSOR),
            )
            .write_buf(contents);
        }
        if self.mode(MODE_BRACKETED_PASTE) != prev.mode(MODE_BRACKETED_PASTE)
        {
            crate::term::BracketedPaste::new(self.mode(MODE_BRACKETED_PASTE))
                .write_buf(contents);
        }
        crate::term::MouseProtocolMode::new(
            self.mouse_protocol_mode,
            prev.mouse_protocol_mode,
        )
        .write_buf(contents);
        crate::term::MouseProtocolEncoding::new(
            self.mouse_protocol_encoding,
            prev.mouse_protocol_encoding,
        )
        .write_buf(contents);
    }

    /// Returns terminal escape sequences sufficient to set the current
    /// terminal's window title.
    #[must_use]
    pub fn title_formatted(&self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_title_formatted(&mut contents);
        contents
    }

    fn write_title_formatted(&self, contents: &mut Vec<u8>) {
        crate::term::ChangeTitle::new(&self.icon_name, &self.title, "", "")
            .write_buf(contents);
    }

    /// Returns terminal escape sequences sufficient to change the previous
    /// terminal's window title to the window title set in the current
    /// terminal.
    #[must_use]
    pub fn title_diff(&self, prev: &Self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_title_diff(&mut contents, prev);
        contents
    }

    fn write_title_diff(&self, contents: &mut Vec<u8>, prev: &Self) {
        crate::term::ChangeTitle::new(
            &self.icon_name,
            &self.title,
            &prev.icon_name,
            &prev.title,
        )
        .write_buf(contents);
    }

    /// Returns terminal escape sequences sufficient to cause audible and
    /// visual bells to occur if they have been received since the terminal
    /// described by `prev`.
    #[must_use]
    pub fn bells_diff(&self, prev: &Self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_bells_diff(&mut contents, prev);
        contents
    }

    fn write_bells_diff(&self, contents: &mut Vec<u8>, prev: &Self) {
        if self.audible_bell_count != prev.audible_bell_count {
            crate::term::AudibleBell::default().write_buf(contents);
        }
        if self.visual_bell_count != prev.visual_bell_count {
            crate::term::VisualBell::default().write_buf(contents);
        }
    }

    /// Returns terminal escape sequences sufficient to set the current
    /// terminal's drawing attributes.
    ///
    /// Supported drawing attributes are:
    /// * fgcolor
    /// * bgcolor
    /// * bold
    /// * italic
    /// * underline
    /// * inverse
    ///
    /// This is not typically necessary, since `contents_formatted` will leave
    /// the current active drawing attributes in the correct state, but this
    /// can be useful in the case of drawing additional things on top of a
    /// terminal output, since you will need to restore the terminal state
    /// without the terminal contents necessarily being the same.
    #[must_use]
    pub fn attributes_formatted(&self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_attributes_formatted(&mut contents);
        contents
    }

    fn write_attributes_formatted(&self, contents: &mut Vec<u8>) {
        crate::term::ClearAttrs::default().write_buf(contents);
        self.attrs.write_escape_code_diff(
            contents,
            &crate::attrs::Attrs::default(),
        );
    }

    /// Returns the current cursor position of the terminal.
    ///
    /// The return value will be (row, col).
    #[must_use]
    pub fn cursor_position(&self) -> (u16, u16) {
        let pos = self.grid().pos();
        (pos.row, pos.col)
    }

    /// Returns terminal escape sequences sufficient to set the current
    /// cursor state of the terminal.
    ///
    /// This is not typically necessary, since `contents_formatted` will leave
    /// the cursor in the correct state, but this can be useful in the case of
    /// drawing additional things on top of a terminal output, since you will
    /// need to restore the terminal state without the terminal contents
    /// necessarily being the same.
    ///
    /// Note that the bytes returned by this function may alter the active
    /// drawing attributes, because it may require redrawing existing cells in
    /// order to position the cursor correctly (for instance, in the case
    /// where the cursor is past the end of a row). Therefore, you should
    /// ensure to reset the active drawing attributes if necessary after
    /// processing this data, for instance by using `attributes_formatted`.
    #[must_use]
    pub fn cursor_state_formatted(&self) -> Vec<u8> {
        let mut contents = vec![];
        self.write_cursor_state_formatted(&mut contents);
        contents
    }

    fn write_cursor_state_formatted(&self, contents: &mut Vec<u8>) {
        crate::term::HideCursor::new(self.hide_cursor()).write_buf(contents);
        self.grid()
            .write_cursor_position_formatted(contents, None, None);

        // we don't just call write_attributes_formatted here, because that
        // would still be confusing - consider the case where the user sets
        // their own unrelated drawing attributes (on a different parser
        // instance) and then calls cursor_state_formatted. just documenting
        // it and letting the user handle it on their own is more
        // straightforward.
    }

    /// Returns the `Cell` object at the given location in the terminal, if it
    /// exists.
    #[must_use]
    pub fn cell(&self, row: u16, col: u16) -> Option<&crate::Cell> {
        self.grid().visible_cell(crate::grid::Pos { row, col })
    }

    /// Returns whether the text in row `row` should wrap to the next line.
    #[must_use]
    pub fn row_wrapped(&self, row: u16) -> bool {
        self.grid()
            .visible_row(row)
            .map_or(false, crate::row::Row::wrapped)
    }

    /// Returns the terminal's window title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the terminal's icon name.
    #[must_use]
    pub fn icon_name(&self) -> &str {
        &self.icon_name
    }

    /// Returns a value which changes every time an audible bell is received.
    ///
    /// Typically you would store this number after each call to `process`,
    /// and trigger an audible bell whenever it changes.
    ///
    /// You shouldn't rely on the exact value returned here, since the exact
    /// value will not be maintained by `contents_formatted` or
    /// `contents_diff`.
    #[must_use]
    pub fn audible_bell_count(&self) -> usize {
        self.audible_bell_count
    }

    /// Returns a value which changes every time an visual bell is received.
    ///
    /// Typically you would store this number after each call to `process`,
    /// and trigger an visual bell whenever it changes.
    ///
    /// You shouldn't rely on the exact value returned here, since the exact
    /// value will not be maintained by `contents_formatted` or
    /// `contents_diff`.
    #[must_use]
    pub fn visual_bell_count(&self) -> usize {
        self.visual_bell_count
    }

    /// Returns the number of parsing errors seen so far.
    ///
    /// Currently this only tracks invalid UTF-8 and control characters other
    /// than `0x07`-`0x0f`. This can give an idea of whether the input stream
    /// being fed to the parser is reasonable or not.
    #[must_use]
    pub fn errors(&self) -> usize {
        self.errors
    }

    /// Returns whether the alternate screen is currently in use.
    #[must_use]
    pub fn alternate_screen(&self) -> bool {
        self.mode(MODE_ALTERNATE_SCREEN)
    }

    /// Returns whether the terminal should be in application keypad mode.
    #[must_use]
    pub fn application_keypad(&self) -> bool {
        self.mode(MODE_APPLICATION_KEYPAD)
    }

    /// Returns whether the terminal should be in application cursor mode.
    #[must_use]
    pub fn application_cursor(&self) -> bool {
        self.mode(MODE_APPLICATION_CURSOR)
    }

    /// Returns whether the terminal should be in hide cursor mode.
    #[must_use]
    pub fn hide_cursor(&self) -> bool {
        self.mode(MODE_HIDE_CURSOR)
    }

    /// Returns whether the terminal should be in bracketed paste mode.
    #[must_use]
    pub fn bracketed_paste(&self) -> bool {
        self.mode(MODE_BRACKETED_PASTE)
    }

    /// Returns the currently active `MouseProtocolMode`
    #[must_use]
    pub fn mouse_protocol_mode(&self) -> MouseProtocolMode {
        self.mouse_protocol_mode
    }

    /// Returns the currently active `MouseProtocolEncoding`
    #[must_use]
    pub fn mouse_protocol_encoding(&self) -> MouseProtocolEncoding {
        self.mouse_protocol_encoding
    }

    /// Returns the currently active foreground color.
    #[must_use]
    pub fn fgcolor(&self) -> crate::Color {
        self.attrs.fgcolor
    }

    /// Returns the currently active background color.
    #[must_use]
    pub fn bgcolor(&self) -> crate::Color {
        self.attrs.bgcolor
    }

    /// Returns whether newly drawn text should be rendered with the bold text
    /// attribute.
    #[must_use]
    pub fn bold(&self) -> bool {
        self.attrs.bold()
    }

    /// Returns whether newly drawn text should be rendered with the italic
    /// text attribute.
    #[must_use]
    pub fn italic(&self) -> bool {
        self.attrs.italic()
    }

    /// Returns whether newly drawn text should be rendered with the
    /// underlined text attribute.
    #[must_use]
    pub fn underline(&self) -> bool {
        self.attrs.underline()
    }

    /// Returns whether newly drawn text should be rendered with the inverse
    /// text attribute.
    #[must_use]
    pub fn inverse(&self) -> bool {
        self.attrs.inverse()
    }

    fn grid(&self) -> &crate::grid::Grid {
        if self.mode(MODE_ALTERNATE_SCREEN) {
            &self.alternate_grid
        } else {
            &self.grid
        }
    }

    fn grid_mut(&mut self) -> &mut crate::grid::Grid {
        if self.mode(MODE_ALTERNATE_SCREEN) {
            &mut self.alternate_grid
        } else {
            &mut self.grid
        }
    }

    fn enter_alternate_grid(&mut self) {
        self.grid_mut().set_scrollback(0);
        self.set_mode(MODE_ALTERNATE_SCREEN);
        self.alternate_grid.allocate_rows();
    }

    fn exit_alternate_grid(&mut self) {
        self.clear_mode(MODE_ALTERNATE_SCREEN);
    }

    fn save_cursor(&mut self) {
        self.grid_mut().save_cursor();
        self.saved_attrs = self.attrs;
    }

    fn restore_cursor(&mut self) {
        self.grid_mut().restore_cursor();
        self.attrs = self.saved_attrs;
    }

    fn set_mode(&mut self, mode: u8) {
        self.modes |= mode;
    }

    fn clear_mode(&mut self, mode: u8) {
        self.modes &= !mode;
    }

    fn mode(&self, mode: u8) -> bool {
        self.modes & mode != 0
    }

    fn set_mouse_mode(&mut self, mode: MouseProtocolMode) {
        self.mouse_protocol_mode = mode;
    }

    fn clear_mouse_mode(&mut self, mode: MouseProtocolMode) {
        if self.mouse_protocol_mode == mode {
            self.mouse_protocol_mode = MouseProtocolMode::default();
        }
    }

    fn set_mouse_encoding(&mut self, encoding: MouseProtocolEncoding) {
        self.mouse_protocol_encoding = encoding;
    }

    fn clear_mouse_encoding(&mut self, encoding: MouseProtocolEncoding) {
        if self.mouse_protocol_encoding == encoding {
            self.mouse_protocol_encoding = MouseProtocolEncoding::default();
        }
    }
}

impl Screen {
    fn text(&mut self, c: char) {
        let pos = self.grid().pos();
        let size = self.grid().size();
        let attrs = self.attrs;

        let width = c.width();
        if width.is_none() && (u32::from(c)) < 256 {
            // don't even try to draw control characters
            return;
        }
        let width = width
            .unwrap_or(1)
            .try_into()
            // width() can only return 0, 1, or 2
            .unwrap();

        // it doesn't make any sense to wrap if the last column in a row
        // didn't already have contents. don't try to handle the case where a
        // character wraps because there was only one column left in the
        // previous row - literally everything handles this case differently,
        // and this is tmux behavior (and also the simplest). i'm open to
        // reconsidering this behavior, but only with a really good reason
        // (xterm handles this by introducing the concept of triple width
        // cells, which i really don't want to do).
        let mut wrap = false;
        if pos.col > size.cols - width {
            let last_cell = self
                .grid()
                .drawing_cell(crate::grid::Pos {
                    row: pos.row,
                    col: size.cols - 1,
                })
                // pos.row is valid, since it comes directly from
                // self.grid().pos() which we assume to always have a valid
                // row value. size.cols - 1 is also always a valid column.
                .unwrap();
            if last_cell.has_contents() || last_cell.is_wide_continuation() {
                wrap = true;
            }
        }
        self.grid_mut().col_wrap(width, wrap);
        let pos = self.grid().pos();

        if width == 0 {
            if pos.col > 0 {
                let mut prev_cell = self
                    .grid_mut()
                    .drawing_cell_mut(crate::grid::Pos {
                        row: pos.row,
                        col: pos.col - 1,
                    })
                    // pos.row is valid, since it comes directly from
                    // self.grid().pos() which we assume to always have a
                    // valid row value. pos.col - 1 is valid because we just
                    // checked for pos.col > 0.
                    .unwrap();
                if prev_cell.is_wide_continuation() {
                    prev_cell = self
                        .grid_mut()
                        .drawing_cell_mut(crate::grid::Pos {
                            row: pos.row,
                            col: pos.col - 2,
                        })
                        // pos.row is valid, since it comes directly from
                        // self.grid().pos() which we assume to always have a
                        // valid row value. we know pos.col - 2 is valid
                        // because the cell at pos.col - 1 is a wide
                        // continuation character, which means there must be
                        // the first half of the wide character before it.
                        .unwrap();
                }
                prev_cell.append(c);
            } else if pos.row > 0 {
                let prev_row = self
                    .grid()
                    .drawing_row(pos.row - 1)
                    // pos.row is valid, since it comes directly from
                    // self.grid().pos() which we assume to always have a
                    // valid row value. pos.row - 1 is valid because we just
                    // checked for pos.row > 0.
                    .unwrap();
                if prev_row.wrapped() {
                    let mut prev_cell = self
                        .grid_mut()
                        .drawing_cell_mut(crate::grid::Pos {
                            row: pos.row - 1,
                            col: size.cols - 1,
                        })
                        // pos.row is valid, since it comes directly from
                        // self.grid().pos() which we assume to always have a
                        // valid row value. pos.row - 1 is valid because we
                        // just checked for pos.row > 0. col of size.cols - 1
                        // is always valid.
                        .unwrap();
                    if prev_cell.is_wide_continuation() {
                        prev_cell = self
                            .grid_mut()
                            .drawing_cell_mut(crate::grid::Pos {
                                row: pos.row - 1,
                                col: size.cols - 2,
                            })
                            // pos.row is valid, since it comes directly from
                            // self.grid().pos() which we assume to always
                            // have a valid row value. pos.row - 1 is valid
                            // because we just checked for pos.row > 0. col of
                            // size.cols - 2 is valid because the cell at
                            // size.cols - 1 is a wide continuation character,
                            // so it must have the first half of the wide
                            // character before it.
                            .unwrap();
                    }
                    prev_cell.append(c);
                }
            }
        } else {
            if self
                .grid()
                .drawing_cell(pos)
                // pos.row is valid because we assume self.grid().pos() to
                // always have a valid row value. pos.col is valid because we
                // called col_wrap() immediately before this, which ensures
                // that self.grid().pos().col has a valid value.
                .unwrap()
                .is_wide_continuation()
            {
                let prev_cell = self
                    .grid_mut()
                    .drawing_cell_mut(crate::grid::Pos {
                        row: pos.row,
                        col: pos.col - 1,
                    })
                    // pos.row is valid because we assume self.grid().pos() to
                    // always have a valid row value. pos.col is valid because
                    // we called col_wrap() immediately before this, which
                    // ensures that self.grid().pos().col has a valid value.
                    // pos.col - 1 is valid because the cell at pos.col is a
                    // wide continuation character, so it must have the first
                    // half of the wide character before it.
                    .unwrap();
                prev_cell.clear(attrs);
            }

            if self
                .grid()
                .drawing_cell(pos)
                // pos.row is valid because we assume self.grid().pos() to
                // always have a valid row value. pos.col is valid because we
                // called col_wrap() immediately before this, which ensures
                // that self.grid().pos().col has a valid value.
                .unwrap()
                .is_wide()
            {
                let next_cell = self
                    .grid_mut()
                    .drawing_cell_mut(crate::grid::Pos {
                        row: pos.row,
                        col: pos.col + 1,
                    })
                    // pos.row is valid because we assume self.grid().pos() to
                    // always have a valid row value. pos.col is valid because
                    // we called col_wrap() immediately before this, which
                    // ensures that self.grid().pos().col has a valid value.
                    // pos.col + 1 is valid because the cell at pos.col is a
                    // wide character, so it must have the second half of the
                    // wide character after it.
                    .unwrap();
                next_cell.set(' ', attrs);
            }

            let cell = self
                .grid_mut()
                .drawing_cell_mut(pos)
                // pos.row is valid because we assume self.grid().pos() to
                // always have a valid row value. pos.col is valid because we
                // called col_wrap() immediately before this, which ensures
                // that self.grid().pos().col has a valid value.
                .unwrap();
            cell.set(c, attrs);
            self.grid_mut().col_inc(1);
            if width > 1 {
                let pos = self.grid().pos();
                if self
                    .grid()
                    .drawing_cell(pos)
                    // pos.row is valid because we assume self.grid().pos() to
                    // always have a valid row value. pos.col is valid because
                    // we called col_wrap() earlier, which ensures that
                    // self.grid().pos().col has a valid value. this is true
                    // even though we just called col_inc, because this branch
                    // only happens if width > 1, and col_wrap takes width
                    // into account.
                    .unwrap()
                    .is_wide()
                {
                    let next_next_pos = crate::grid::Pos {
                        row: pos.row,
                        col: pos.col + 1,
                    };
                    let next_next_cell = self
                        .grid_mut()
                        .drawing_cell_mut(next_next_pos)
                        // pos.row is valid because we assume
                        // self.grid().pos() to always have a valid row value.
                        // pos.col is valid because we called col_wrap()
                        // earlier, which ensures that self.grid().pos().col
                        // has a valid value. this is true even though we just
                        // called col_inc, because this branch only happens if
                        // width > 1, and col_wrap takes width into account.
                        // pos.col + 1 is valid because the cell at pos.col is
                        // wide, and so it must have the second half of the
                        // wide character after it.
                        .unwrap();
                    next_next_cell.clear(attrs);
                    if next_next_pos.col == size.cols - 1 {
                        self.grid_mut()
                            .drawing_row_mut(pos.row)
                            // we assume self.grid().pos().row is always valid
                            .unwrap()
                            .wrap(false);
                    }
                }
                let next_cell = self
                    .grid_mut()
                    .drawing_cell_mut(pos)
                    // pos.row is valid because we assume self.grid().pos() to
                    // always have a valid row value. pos.col is valid because
                    // we called col_wrap() earlier, which ensures that
                    // self.grid().pos().col has a valid value. this is true
                    // even though we just called col_inc, because this branch
                    // only happens if width > 1, and col_wrap takes width
                    // into account.
                    .unwrap();
                next_cell.clear(crate::attrs::Attrs::default());
                next_cell.set_wide_continuation(true);
                self.grid_mut().col_inc(1);
            }
        }
    }

    // control codes

    fn bel(&mut self) {
        self.audible_bell_count += 1;
    }

    fn bs(&mut self) {
        self.grid_mut().col_dec(1);
    }

    fn tab(&mut self) {
        self.grid_mut().col_tab();
    }

    fn lf(&mut self) {
        self.grid_mut().row_inc_scroll(1);
    }

    fn vt(&mut self) {
        self.lf();
    }

    fn ff(&mut self) {
        self.lf();
    }

    fn cr(&mut self) {
        self.grid_mut().col_set(0);
    }

    // escape codes

    // ESC 7
    fn decsc(&mut self) {
        self.save_cursor();
    }

    // ESC 8
    fn decrc(&mut self) {
        self.restore_cursor();
    }

    // ESC =
    fn deckpam(&mut self) {
        self.set_mode(MODE_APPLICATION_KEYPAD);
    }

    // ESC >
    fn deckpnm(&mut self) {
        self.clear_mode(MODE_APPLICATION_KEYPAD);
    }

    // ESC M
    fn ri(&mut self) {
        self.grid_mut().row_dec_scroll(1);
    }

    // ESC c
    fn ris(&mut self) {
        let title = self.title.clone();
        let icon_name = self.icon_name.clone();
        let audible_bell_count = self.audible_bell_count;
        let visual_bell_count = self.visual_bell_count;
        let errors = self.errors;

        *self = Self::new(self.grid.size(), self.grid.scrollback_len());

        self.title = title;
        self.icon_name = icon_name;
        self.audible_bell_count = audible_bell_count;
        self.visual_bell_count = visual_bell_count;
        self.errors = errors;
    }

    // ESC g
    fn vb(&mut self) {
        self.visual_bell_count += 1;
    }

    // csi codes

    // CSI @
    fn ich(&mut self, count: u16) {
        self.grid_mut().insert_cells(count);
    }

    // CSI A
    fn cuu(&mut self, offset: u16) {
        self.grid_mut().row_dec_clamp(offset);
    }

    // CSI B
    fn cud(&mut self, offset: u16) {
        self.grid_mut().row_inc_clamp(offset);
    }

    // CSI C
    fn cuf(&mut self, offset: u16) {
        self.grid_mut().col_inc_clamp(offset);
    }

    // CSI D
    fn cub(&mut self, offset: u16) {
        self.grid_mut().col_dec(offset);
    }

    // CSI G
    fn cha(&mut self, col: u16) {
        self.grid_mut().col_set(col - 1);
    }

    // CSI H
    fn cup(&mut self, (row, col): (u16, u16)) {
        self.grid_mut().set_pos(crate::grid::Pos {
            row: row - 1,
            col: col - 1,
        });
    }

    // CSI J
    fn ed(&mut self, mode: u16) {
        let attrs = self.attrs;
        match mode {
            0 => self.grid_mut().erase_all_forward(attrs),
            1 => self.grid_mut().erase_all_backward(attrs),
            2 => self.grid_mut().erase_all(attrs),
            n => {
                log::debug!("unhandled ED mode: {n}");
            }
        }
    }

    // CSI ? J
    fn decsed(&mut self, mode: u16) {
        self.ed(mode);
    }

    // CSI K
    fn el(&mut self, mode: u16) {
        let attrs = self.attrs;
        match mode {
            0 => self.grid_mut().erase_row_forward(attrs),
            1 => self.grid_mut().erase_row_backward(attrs),
            2 => self.grid_mut().erase_row(attrs),
            n => {
                log::debug!("unhandled EL mode: {n}");
            }
        }
    }

    // CSI ? K
    fn decsel(&mut self, mode: u16) {
        self.el(mode);
    }

    // CSI L
    fn il(&mut self, count: u16) {
        self.grid_mut().insert_lines(count);
    }

    // CSI M
    fn dl(&mut self, count: u16) {
        self.grid_mut().delete_lines(count);
    }

    // CSI P
    fn dch(&mut self, count: u16) {
        self.grid_mut().delete_cells(count);
    }

    // CSI S
    fn su(&mut self, count: u16) {
        self.grid_mut().scroll_up(count);
    }

    // CSI T
    fn sd(&mut self, count: u16) {
        self.grid_mut().scroll_down(count);
    }

    // CSI X
    fn ech(&mut self, count: u16) {
        let attrs = self.attrs;
        self.grid_mut().erase_cells(count, attrs);
    }

    // CSI d
    fn vpa(&mut self, row: u16) {
        self.grid_mut().row_set(row - 1);
    }

    // CSI h
    #[allow(clippy::unused_self)]
    fn sm(&mut self, params: &vte::Params) {
        // nothing, i think?
        if log::log_enabled!(log::Level::Debug) {
            log::debug!("unhandled SM mode: {}", param_str(params));
        }
    }

    // CSI ? h
    fn decset(&mut self, params: &vte::Params) {
        for param in params {
            match param {
                &[1] => self.set_mode(MODE_APPLICATION_CURSOR),
                &[6] => self.grid_mut().set_origin_mode(true),
                &[9] => self.set_mouse_mode(MouseProtocolMode::Press),
                &[25] => self.clear_mode(MODE_HIDE_CURSOR),
                &[47] => self.enter_alternate_grid(),
                &[1000] => {
                    self.set_mouse_mode(MouseProtocolMode::PressRelease);
                }
                &[1002] => {
                    self.set_mouse_mode(MouseProtocolMode::ButtonMotion);
                }
                &[1003] => self.set_mouse_mode(MouseProtocolMode::AnyMotion),
                &[1005] => {
                    self.set_mouse_encoding(MouseProtocolEncoding::Utf8);
                }
                &[1006] => {
                    self.set_mouse_encoding(MouseProtocolEncoding::Sgr);
                }
                &[1049] => {
                    self.decsc();
                    self.alternate_grid.clear();
                    self.enter_alternate_grid();
                }
                &[2004] => self.set_mode(MODE_BRACKETED_PASTE),
                ns => {
                    if log::log_enabled!(log::Level::Debug) {
                        let n = if ns.len() == 1 {
                            format!(
                                "{}",
                                // we just checked that ns.len() == 1, so 0
                                // must be valid
                                ns[0]
                            )
                        } else {
                            format!("{ns:?}")
                        };
                        log::debug!("unhandled DECSET mode: {n}");
                    }
                }
            }
        }
    }

    // CSI l
    #[allow(clippy::unused_self)]
    fn rm(&mut self, params: &vte::Params) {
        // nothing, i think?
        if log::log_enabled!(log::Level::Debug) {
            log::debug!("unhandled RM mode: {}", param_str(params));
        }
    }

    // CSI ? l
    fn decrst(&mut self, params: &vte::Params) {
        for param in params {
            match param {
                &[1] => self.clear_mode(MODE_APPLICATION_CURSOR),
                &[6] => self.grid_mut().set_origin_mode(false),
                &[9] => self.clear_mouse_mode(MouseProtocolMode::Press),
                &[25] => self.set_mode(MODE_HIDE_CURSOR),
                &[47] => {
                    self.exit_alternate_grid();
                }
                &[1000] => {
                    self.clear_mouse_mode(MouseProtocolMode::PressRelease);
                }
                &[1002] => {
                    self.clear_mouse_mode(MouseProtocolMode::ButtonMotion);
                }
                &[1003] => {
                    self.clear_mouse_mode(MouseProtocolMode::AnyMotion);
                }
                &[1005] => {
                    self.clear_mouse_encoding(MouseProtocolEncoding::Utf8);
                }
                &[1006] => {
                    self.clear_mouse_encoding(MouseProtocolEncoding::Sgr);
                }
                &[1049] => {
                    self.exit_alternate_grid();
                    self.decrc();
                }
                &[2004] => self.clear_mode(MODE_BRACKETED_PASTE),
                ns => {
                    if log::log_enabled!(log::Level::Debug) {
                        let n = if ns.len() == 1 {
                            format!(
                                "{}",
                                // we just checked that ns.len() == 1, so 0
                                // must be valid
                                ns[0]
                            )
                        } else {
                            format!("{ns:?}")
                        };
                        log::debug!("unhandled DECRST mode: {n}");
                    }
                }
            }
        }
    }

    // CSI m
    fn sgr(&mut self, params: &vte::Params) {
        // XXX really i want to just be able to pass in a default Params
        // instance with a 0 in it, but vte doesn't allow creating new Params
        // instances
        if params.is_empty() {
            self.attrs = crate::attrs::Attrs::default();
            return;
        }

        let mut iter = params.iter();

        macro_rules! next_param {
            () => {
                match iter.next() {
                    Some(n) => n,
                    _ => return,
                }
            };
        }

        macro_rules! to_u8 {
            ($n:expr) => {
                if let Some(n) = u16_to_u8($n) {
                    n
                } else {
                    return;
                }
            };
        }

        macro_rules! next_param_u8 {
            () => {
                if let &[n] = next_param!() {
                    to_u8!(n)
                } else {
                    return;
                }
            };
        }

        loop {
            match next_param!() {
                &[0] => self.attrs = crate::attrs::Attrs::default(),
                &[1] => self.attrs.set_bold(true),
                &[3] => self.attrs.set_italic(true),
                &[4] => self.attrs.set_underline(true),
                &[7] => self.attrs.set_inverse(true),
                &[22] => self.attrs.set_bold(false),
                &[23] => self.attrs.set_italic(false),
                &[24] => self.attrs.set_underline(false),
                &[27] => self.attrs.set_inverse(false),
                &[n] if (30..=37).contains(&n) => {
                    self.attrs.fgcolor = crate::Color::Idx(to_u8!(n) - 30);
                }
                &[38, 2, r, g, b] => {
                    self.attrs.fgcolor =
                        crate::Color::Rgb(to_u8!(r), to_u8!(g), to_u8!(b));
                }
                &[38, 5, i] => {
                    self.attrs.fgcolor = crate::Color::Idx(to_u8!(i));
                }
                &[38] => match next_param!() {
                    &[2] => {
                        let r = next_param_u8!();
                        let g = next_param_u8!();
                        let b = next_param_u8!();
                        self.attrs.fgcolor = crate::Color::Rgb(r, g, b);
                    }
                    &[5] => {
                        self.attrs.fgcolor =
                            crate::Color::Idx(next_param_u8!());
                    }
                    ns => {
                        if log::log_enabled!(log::Level::Debug) {
                            let n = if ns.len() == 1 {
                                format!(
                                    "{}",
                                    // we just checked that ns.len() == 1, so
                                    // 0 must be valid
                                    ns[0]
                                )
                            } else {
                                format!("{ns:?}")
                            };
                            log::debug!("unhandled SGR mode: 38 {n}");
                        }
                        return;
                    }
                },
                &[39] => {
                    self.attrs.fgcolor = crate::Color::Default;
                }
                &[n] if (40..=47).contains(&n) => {
                    self.attrs.bgcolor = crate::Color::Idx(to_u8!(n) - 40);
                }
                &[48, 2, r, g, b] => {
                    self.attrs.bgcolor =
                        crate::Color::Rgb(to_u8!(r), to_u8!(g), to_u8!(b));
                }
                &[48, 5, i] => {
                    self.attrs.bgcolor = crate::Color::Idx(to_u8!(i));
                }
                &[48] => match next_param!() {
                    &[2] => {
                        let r = next_param_u8!();
                        let g = next_param_u8!();
                        let b = next_param_u8!();
                        self.attrs.bgcolor = crate::Color::Rgb(r, g, b);
                    }
                    &[5] => {
                        self.attrs.bgcolor =
                            crate::Color::Idx(next_param_u8!());
                    }
                    ns => {
                        if log::log_enabled!(log::Level::Debug) {
                            let n = if ns.len() == 1 {
                                format!(
                                    "{}",
                                    // we just checked that ns.len() == 1, so
                                    // 0 must be valid
                                    ns[0]
                                )
                            } else {
                                format!("{ns:?}")
                            };
                            log::debug!("unhandled SGR mode: 48 {n}");
                        }
                        return;
                    }
                },
                &[49] => {
                    self.attrs.bgcolor = crate::Color::Default;
                }
                &[n] if (90..=97).contains(&n) => {
                    self.attrs.fgcolor = crate::Color::Idx(to_u8!(n) - 82);
                }
                &[n] if (100..=107).contains(&n) => {
                    self.attrs.bgcolor = crate::Color::Idx(to_u8!(n) - 92);
                }
                ns => {
                    if log::log_enabled!(log::Level::Debug) {
                        let n = if ns.len() == 1 {
                            format!(
                                "{}",
                                // we just checked that ns.len() == 1, so 0
                                // must be valid
                                ns[0]
                            )
                        } else {
                            format!("{ns:?}")
                        };
                        log::debug!("unhandled SGR mode: {n}");
                    }
                }
            }
        }
    }

    // CSI r
    fn decstbm(&mut self, (top, bottom): (u16, u16)) {
        self.grid_mut().set_scroll_region(top - 1, bottom - 1);
    }

    // osc codes

    fn osc0(&mut self, s: &[u8]) {
        self.osc1(s);
        self.osc2(s);
    }

    fn osc1(&mut self, s: &[u8]) {
        if let Ok(s) = std::str::from_utf8(s) {
            self.icon_name = s.to_string();
        }
    }

    fn osc2(&mut self, s: &[u8]) {
        if let Ok(s) = std::str::from_utf8(s) {
            self.title = s.to_string();
        }
    }
}

impl vte::Perform for Screen {
    fn print(&mut self, c: char) {
        if c == '\u{fffd}' || ('\u{80}'..'\u{a0}').contains(&c) {
            self.errors = self.errors.saturating_add(1);
        }
        self.text(c);
    }

    fn execute(&mut self, b: u8) {
        match b {
            7 => self.bel(),
            8 => self.bs(),
            9 => self.tab(),
            10 => self.lf(),
            11 => self.vt(),
            12 => self.ff(),
            13 => self.cr(),
            // we don't implement shift in/out alternate character sets, but
            // it shouldn't count as an "error"
            14 | 15 => {}
            _ => {
                self.errors = self.errors.saturating_add(1);
                log::debug!("unhandled control character: {b}");
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, b: u8) {
        intermediates.first().map_or_else(
            || match b {
                b'7' => self.decsc(),
                b'8' => self.decrc(),
                b'=' => self.deckpam(),
                b'>' => self.deckpnm(),
                b'M' => self.ri(),
                b'c' => self.ris(),
                b'g' => self.vb(),
                _ => {
                    log::debug!("unhandled escape code: ESC {b}");
                }
            },
            |i| {
                log::debug!("unhandled escape code: ESC {i} {b}");
            },
        );
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        c: char,
    ) {
        match intermediates.first() {
            None => match c {
                '@' => self.ich(canonicalize_params_1(params, 1)),
                'A' => self.cuu(canonicalize_params_1(params, 1)),
                'B' => self.cud(canonicalize_params_1(params, 1)),
                'C' => self.cuf(canonicalize_params_1(params, 1)),
                'D' => self.cub(canonicalize_params_1(params, 1)),
                'G' => self.cha(canonicalize_params_1(params, 1)),
                'H' => self.cup(canonicalize_params_2(params, 1, 1)),
                'J' => self.ed(canonicalize_params_1(params, 0)),
                'K' => self.el(canonicalize_params_1(params, 0)),
                'L' => self.il(canonicalize_params_1(params, 1)),
                'M' => self.dl(canonicalize_params_1(params, 1)),
                'P' => self.dch(canonicalize_params_1(params, 1)),
                'S' => self.su(canonicalize_params_1(params, 1)),
                'T' => self.sd(canonicalize_params_1(params, 1)),
                'X' => self.ech(canonicalize_params_1(params, 1)),
                'd' => self.vpa(canonicalize_params_1(params, 1)),
                'h' => self.sm(params),
                'l' => self.rm(params),
                'm' => self.sgr(params),
                'r' => self.decstbm(canonicalize_params_decstbm(
                    params,
                    self.grid().size(),
                )),
                _ => {
                    if log::log_enabled!(log::Level::Debug) {
                        log::debug!(
                            "unhandled csi sequence: CSI {} {}",
                            param_str(params),
                            c
                        );
                    }
                }
            },
            Some(b'?') => match c {
                'J' => self.decsed(canonicalize_params_1(params, 0)),
                'K' => self.decsel(canonicalize_params_1(params, 0)),
                'h' => self.decset(params),
                'l' => self.decrst(params),
                _ => {
                    if log::log_enabled!(log::Level::Debug) {
                        log::debug!(
                            "unhandled csi sequence: CSI ? {} {}",
                            param_str(params),
                            c
                        );
                    }
                }
            },
            Some(i) => {
                if log::log_enabled!(log::Level::Debug) {
                    log::debug!(
                        "unhandled csi sequence: CSI {} {} {}",
                        i,
                        param_str(params),
                        c
                    );
                }
            }
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bel_terminated: bool) {
        match (params.get(0), params.get(1)) {
            (Some(&b"0"), Some(s)) => self.osc0(s),
            (Some(&b"1"), Some(s)) => self.osc1(s),
            (Some(&b"2"), Some(s)) => self.osc2(s),
            _ => {
                if log::log_enabled!(log::Level::Debug) {
                    log::debug!(
                        "unhandled osc sequence: OSC {}",
                        osc_param_str(params),
                    );
                }
            }
        }
    }

    fn hook(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if log::log_enabled!(log::Level::Debug) {
            intermediates.first().map_or_else(
                || {
                    log::debug!(
                        "unhandled dcs sequence: DCS {} {}",
                        param_str(params),
                        action,
                    );
                },
                |i| {
                    log::debug!(
                        "unhandled dcs sequence: DCS {} {} {}",
                        i,
                        param_str(params),
                        action,
                    );
                },
            );
        }
    }
}

fn canonicalize_params_1(params: &vte::Params, default: u16) -> u16 {
    let first = params.iter().next().map_or(0, |x| *x.first().unwrap_or(&0));
    if first == 0 {
        default
    } else {
        first
    }
}

fn canonicalize_params_2(
    params: &vte::Params,
    default1: u16,
    default2: u16,
) -> (u16, u16) {
    let mut iter = params.iter();
    let first = iter.next().map_or(0, |x| *x.first().unwrap_or(&0));
    let first = if first == 0 { default1 } else { first };

    let second = iter.next().map_or(0, |x| *x.first().unwrap_or(&0));
    let second = if second == 0 { default2 } else { second };

    (first, second)
}

fn canonicalize_params_decstbm(
    params: &vte::Params,
    size: crate::grid::Size,
) -> (u16, u16) {
    let mut iter = params.iter();
    let top = iter.next().map_or(0, |x| *x.first().unwrap_or(&0));
    let top = if top == 0 { 1 } else { top };

    let bottom = iter.next().map_or(0, |x| *x.first().unwrap_or(&0));
    let bottom = if bottom == 0 { size.rows } else { bottom };

    (top, bottom)
}

fn u16_to_u8(i: u16) -> Option<u8> {
    if i > u16::from(u8::max_value()) {
        None
    } else {
        // safe because we just ensured that the value fits in a u8
        Some(i.try_into().unwrap())
    }
}

fn param_str(params: &vte::Params) -> String {
    let strs: Vec<_> = params
        .iter()
        .map(|subparams| {
            let subparam_strs: Vec<_> = subparams
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
            subparam_strs.join(" : ")
        })
        .collect();
    strs.join(" ; ")
}

fn osc_param_str(params: &[&[u8]]) -> String {
    let strs: Vec<_> = params
        .iter()
        .map(|b| format!("\"{}\"", std::string::String::from_utf8_lossy(b)))
        .collect();
    strs.join(" ; ")
}
