//! Manages the data that will be displayed to the user.
use crate::{
    control::{Edge, ParseAbilityError, Ability, Command},
    ui::{self, Address, Change, Color, Span, CHAR_BACKSPACE, CHAR_ENTER},
};
use core::{num::NonZeroUsize, iter};
use lazy_static::lazy_static;
use lsp_types::TextDocumentItem;
use rec::{Class, var, Pattern};
use std::{
    cmp,
    fmt::{self, Display, Formatter},
    fs, io,
    ops::Deref,
    path::PathBuf,
};
use url::Url;

/// Defines a result that will be processed by the application.
///
/// This is a [`Result`] of an [`Option`], where the [`Err`] has 2 variants, meaning there are 4
/// categories of `Directive`s:
/// 1. "Output" `Ok(Some(Vec<Change>))` = Application forwards [`Change`]s to user interface.
/// 2. "Quit" `Ok(None)`= Application quits with no error.
/// 3. "Issue" `Err(Error::Issue)` = Application resolves error.
/// 4. "Crash" `Err(Error::Crash)` = Application quits with error.
pub(crate) type Directive = Result<Option<Vec<Change>>, Error>;

/// Returns an "Output" [`Directive`] with no [`Change`]s.
pub(crate) fn empty_directive() -> Directive {
    Ok(Some(Vec::new()))
}

/// Manages everything currently visible to the user.
///
/// All edits to the output are performed by returning an "Output" [`Directive`] with the
/// appropriate [`Change`]s.
///
/// For now only 1 `Sheet` is created per [`Paper`] and each `Sheet` can only hold 1 `Pane`.
#[derive(Clone, Debug)]
pub(crate) struct Sheet {
    /// The [`Pane`].
    pane: Pane,
    /// The [`ControlPanel`].
    control_panel: ControlPanel,
}

impl Sheet {
    /// Creates a new [`Sheet`].
    ///
    /// `base_url` is the directory where the application was started. `height` is the height of
    /// the sheet in number of cells.
    pub(crate) fn new(base_url: Url, height: NonZeroUsize) -> Self {
        Self {
            pane: Pane::new(base_url, height),
            // Because height is not 0, height - 1 will never wrap past boundary.
            control_panel: ControlPanel::new(height.get().saturating_sub(1)),
        }
    }

    /// Scrolls the [`Pane`] down.
    pub(crate) fn scroll_down(&mut self) -> Directive {
        self.pane.scroll_down()
    }

    /// Scrolls the [`Pane`] up.
    pub(crate) fn scroll_up(&mut self) -> Directive {
        self.pane.scroll_up()
    }

    /// Opens the [`ControlPanel`] and writes `abiility_char` to it.
    pub(crate) fn open_control_panel(&mut self, ability_char: char) -> Directive {
        self.control_panel.clear();
        self.write_to_control_panel(ability_char)
    }

    /// Writes `input` to the [`ControlPanel`], formatting the [`Pane`] based on the new input.
    pub(crate) fn write_to_control_panel(&mut self, input: char) -> Directive {
        self.control_panel.write(input)?;
        self.pane.process_ability(&self.control_panel.ability);
        self.refresh()
    }

    /// Writes `input` to the [`Pane`].
    pub(crate) fn write_to_pane(&mut self, input: char) -> Directive {
        self.pane.write(input)
    }

    /// Performs the [`Ability`] specified in the [`ControlPanel`].
    pub(crate) fn perform_ability(&mut self) -> Directive {
        match &self.control_panel.ability {
            Ability::Command(Some(command)) => match command {
                Command::See(path) => self
                    .pane
                    .open(self.pane.base_url.join(path).map_err(|_| Error::Issue(Issue::InvalidPath))?),
                Command::Put => self.pane.save(),
                Command::End => Ok(None),
                Command::Unknown(_) => Err(Error::Issue(Issue::InvalidAbility)),
            }
            _ => empty_directive(),
        }
    }

    /// Removes the [`ControlPanel`] from the output.
    pub(crate) fn close_control_panel(&mut self) -> Directive {
        self.control_panel.clear();
        self.refresh()
    }

    /// Changes the [`Selection`]s of the [`Pane`] as specified by `edge`.
    pub(crate) fn select_edge(&mut self, edge: Edge) -> Directive {
        self.pane.select_edge(edge)
    }

    /// Flashes the output.
    pub(crate) fn flash(&self) -> Directive {
        Ok(Some(vec![Change::Flash]))
    }

    /// Updates the [`ControlPanel`] and [`Pane`] formatting.
    fn refresh(&self) -> Directive {
        self.pane.output_with_repaint(self.control_panel.changes())
    }
}

/// Manages the output of a [`Document`].
#[derive(Clone, Debug)]
pub(crate) struct Pane {
    /// The [`Docuement`].
    doc: Document,
    /// The index of the first string of `doc` that is displayed.
    first_visible_string_index: usize,
    /// The number of columns needed to display the margin.
    ///
    /// This does not include any space to separate the margin from the text.
    margin_width: usize,
    /// The number of visible rows.
    height: NonZeroUsize,
    /// The directory to which all relative paths will be joined.
    base_url: Url,
    /// The current [`Selection`]s.
    selections: Vec<Selection>,
    /// The current [`Selection`]s.
    old_selections: Vec<OldSelection>,
}

impl Pane {
    /// Creates a new `Pane`.
    fn new(base_url: Url, height: NonZeroUsize) -> Self {
        // Cannot use ..Default::default() due to Url not impl Default.
        Self {
            base_url,
            height,
            doc: Document::default(),
            first_visible_string_index: usize::default(),
            margin_width: usize::default(),
            selections: Vec::default(),
            old_selections: Vec::default(),
        }
    }

    /// Scrolls the view of [`Document`] down.
    fn scroll_down(&mut self) -> Directive {
        self.jump_to(cmp::min(
            self.first_visible_string_index.saturating_add(self.scroll_delta()),
            self.doc.string_count().saturating_sub(1),
        ))
    }

    /// Scrolls the view of [`Document`] up.
    fn scroll_up(&mut self) -> Directive {
        self.jump_to(self.first_visible_string_index.saturating_sub(self.scroll_delta()))
    }

    /// Sets the first visible string to `index`, updating the output if needed.
    fn jump_to(&mut self, index: usize) -> Directive {
        if index == self.first_visible_string_index {
            empty_directive()
        } else {
            self.first_visible_string_index = index;
            self.redraw()
        }
    }

    /// Returns the number of strings that a scroll movement should move.
    fn scroll_delta(&self) -> usize {
        self.height.get().wrapping_div(4)
    }

    /// Redraws `self` to output.
    ///
    /// Assumes the [`ControlPanel`] is closed.
    fn redraw(&self) -> Directive {
        self.output_with_repaint(iter::once(Change::Clear)
            .chain(
                self.doc
                    .strings()
                    .skip(self.first_visible_string_index)
                    .take(self.height.get())
                    .enumerate()
                    .map(|(row, (line_number, line))|
                        Change::Write {
                            span: Span::entire_row(row),
                            text: format!(
                                "{: >width$} {}",
                                line_number,
                                line,
                                width = self.margin_width
                            ),
                        }
                    ),
            )
            .collect())
    }

    /// Outputs `initial_changes` followed by [`Pane::repaint`].
    fn output_with_repaint(&self, mut initial_changes: Vec<Change>) -> Directive {
        self.repaint().map(|result| result.map(|changes| {
            initial_changes.extend(changes);
            initial_changes
        }))
    }

    /// Updates the formatting of `self`.
    fn repaint(&self) -> Directive {
        let mut changes: Vec<Change> = (0..self.height.get())
            .map(|row| Change::Format{span: Span::entire_row(row), color: Color::Black})
            .collect();

        for selection in &self.old_selections {
            if let Some(span) = self.span_at(selection) {
                changes.push(Change::Format{span, color: Color::Blue});
            }
        }

        Ok(Some(changes))
    }

    /// Changes the [`Selection`]s of `self` as specified by `edge`.
    pub(crate) fn select_edge(&mut self, edge: Edge) -> Directive {
        for selection in &mut self.old_selections {
            selection.select_edge(edge);
        }

        self.repaint()
    }

    /// Updates [`Selection`]s of `self` based on `ability`.
    fn process_ability(&mut self, ability: &Ability) {
        let mut index = 0;
        self.selections.clear();

        for line in 0..self.doc.string_count() {
            if let Some(string) = self.doc.line(line) {
                let len = string.len();
                self.selections.push(Selection::with_len(index, len));
                index += len + 1;
            }
        }

        match ability {
            Ability::Pattern(Some(pattern)) => {
                let target_selections = self.old_selections.clone();
                self.old_selections.clear();

                for target_selection in target_selections {
                    let target_line = target_selection.head.string();
                    if let Some(target) = self
                        .doc
                        .line(target_line)
                        .map(|x| x.chars().skip(target_selection.head.shift).collect::<String>())
                    {
                        for location in pattern.find_iter(&target) {
                            let line = target_selection.head.string;
                            self.old_selections.push(OldSelection::new(
                                Location::new(line, location.start()),
                                Location::new(line, location.end()),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Changes the pane to a new path.
    fn open(&mut self, url: Url) -> Directive {
        self.doc = Document::new(url)?;
        self.refresh();
        self.redraw()
    }

    /// Saves the document of `Pane` to its file system.
    fn save(&self) -> Directive {
        self.doc.save()?;
        empty_directive()
    }

    /// Refresh
    fn refresh(&mut self) {
        self.margin_width = self.doc.string_count().to_string().len();
    }

    /// Adds a character at a [`Position`].
    pub(crate) fn write(&mut self, input: char) -> Directive {
        let mut new_selections = Vec::new();
        let mut changes = Vec::new();
        let mut will_redraw = false;

        for a_selection in self.old_selections.clone() {
            let mut selection = a_selection;
            let mut new_text = String::new();

            if input == CHAR_BACKSPACE {
                if selection.starts_at_first_shift() {
                    if !selection.starts_at_first_string() {
                        will_redraw = true;
                    }
                } else {
                    selection.expand_left();

                    if let Some(span) = self.span_at(&selection) {
                        changes.push(Change::Write{span, text: new_text.clone()});
                    }
                }
            } else {
                new_text.push(input);

                if input == CHAR_ENTER {
                    will_redraw = true;
                } else if let Some(span) = self.span_at(&selection) {
                    changes.push(Change::Write{span, text: new_text.clone()});
                } else {
                    // Do nothing.
                }
            }

            self.doc.edit(&mut selection, input)?;
            self.refresh();
            new_selections.push(selection)
        }

        self.old_selections = new_selections;

        if will_redraw {
            self.redraw()
        } else {
            self.output_with_repaint(changes)
        }
    }

    /// Returns the row at which a [`Position`] is located.
    ///
    /// [`None`] indicates that the [`Position`] is not visible in the user interface.
    fn row_at(&self, position: &Location) -> Option<usize> {
        position.string_diff(self.first_visible_string_index.saturating_add(1))
    }

    /// Returns the column at which a [`Position`] is located.
    #[allow(clippy::missing_const_for_fn)] // saturating_add is not yet stable as a const fn.
    fn column_at(&self, position: &Location) -> usize {
        position.shift_add(self.margin_width.saturating_add(1))
    }

    /// Returns the [`Address`] associated with the given [`Position`].
    fn address_at(&self, position: &Location) -> Option<Address> {
        self.row_at(&position)
            .and_then(|row| Some(Address::new(row, self.column_at(&position))))
    }

    /// Returns the `Span` associated with the given `Range`.
    fn span_at(&self, range: &OldSelection) -> Option<Span> {
        self.address_at(&range.head).and_then(|first| {
            self.address_at(&range.tail)
                .map(|last| Span::new(first, last))
        })
    }
}

#[derive(Clone, Debug)]
struct Selection {
    head: usize,
    tail: usize,
}

impl Selection {
    fn new(head: usize, tail: usize) -> Self {
        Self {
            head,
            tail,
        }
    }

    fn with_len(head: usize, len: usize) -> Self {
        Self {
            head,
            tail: head + len,
        }
    }
}

/// A `Location` within a `Document`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Location {
    /// string
    string: usize,
    /// shift
    shift: usize,
}

impl Location {
    /// new
    const fn new(string: usize, shift: usize) -> Self {
        Self {
            string,
            shift,
        }
    }

    /// Moves `Location` left.
    fn move_left(&mut self) {
        self.shift -= 1;
    }

    /// Moves `Location` right.
    fn move_right(&mut self) {
        self.shift += 1;
    }

    /// Moves `Location` to end of line.
    fn move_to_end(&mut self) {
        self.shift = usize::max_value();
    }

    /// Moves `Location` up.
    fn move_up(&mut self) {
        self.string -= 1;
    }

    /// If the `Location` is in the first line of the column.
    const fn is_first_string(&self) -> bool {
        self.string == 0
    }

    /// If the `Location` is in the first column of its line.
    const fn is_first_shift(&self) -> bool {
        self.shift == 0
    }

    pub(crate) fn string_diff(&self, string: usize) -> Option<usize> {
        self.string.checked_sub(string)
    }

    pub(crate) fn shift_add(&self, shift: usize) -> usize {
        self.shift.saturating_add(shift)
    }

    pub(crate) fn string(&self) -> usize {
        self.string
    }

    pub(crate) fn shift(&self) -> usize {
        self.shift
    }
}

/// A series of [`Location`]s.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OldSelection {
    /// The first [`Location`] in `Selection`.
    head: Location,
    /// The last [`Location`] in `Selection`.
    tail: Location,
}

impl OldSelection {
    /// new
    const fn new(head: Location, tail: Location) -> Self {
        Self {
            head,
            tail,
        }
    }

    /// Entire line
    pub(crate) const fn entire_line(line: usize) -> Self {
        Self {
            head: Location::new(line, 0),
            tail: Location::new(line, usize::max_value()),
        }
    }

    /// select edge
    pub(crate) fn select_edge(&mut self, edge: Edge) {
        match edge {
            Edge::Start => {
                self.tail = self.head;
            }
            Edge::End => {
                self.head = self.tail;
            }
        }
    }

    /// doc
    pub(crate) fn starts_at_first_shift(&self) -> bool {
        self.head.is_first_shift()
    }

    /// doc
    pub(crate) fn starts_at_first_string(&self) -> bool {
        self.head.is_first_string()
    }

    /// doc
    pub(crate) fn expand_left(&mut self) {
        self.head.move_left()
    }

    /// doc
    pub(crate) fn expand_up(&mut self) {
        self.head.move_up()
    }

    /// doc
    pub(crate) fn move_head_to_end(&mut self) {
        self.head.move_to_end()
    }

    /// doc
    pub(crate) fn move_right(&mut self) {
        self.head.move_right();
        self.tail.move_right();
    }
}

/// Signifies a document.
#[derive(Clone, Debug, Default)]
struct Document {
    /// Information about `Document` used for LanguageServerProtocol.
    item: Option<TextDocumentItem>,
    /// The path of the `Document`.
    path: PathBuf,
    /// The number of strings in `Document`.
    string_count: usize,
}

impl Document {
    /// Creates a new `Document`.
    fn new(url: Url) -> Result<Self, Error> {
        let path = url
            .to_file_path()
            .map_err(|_| Error::Issue(Issue::InvalidPath))?;
        fs::read_to_string(&path)
            .map(|text| Self {
                item: Some(TextDocumentItem::new(
                    url,
                    "rust".to_string(),
                    0,
                    text.replace('\r', ""),
                )),
                path,
                string_count: text.lines().count(),
            })
            .map_err(Error::from)
    }

    /// Writes the text of `Document` to the filesystem.
    fn save(&self) -> Result<(), Error> {
        if let Some(item) = &self.item {
            fs::write(&self.path, &item.text).map_err(Error::from)
        } else {
            Ok(())
        }
    }

    /// Returns the character index of `location` within `Document`.
    fn index_of(&self, location: &Location) -> Result<usize, Error> {
        lazy_static! {
            static ref LINE_PATTERN: Pattern = Pattern::new(var(Class::Any) + '\n');
        }

        if let Some(item) = &self.item {
            LINE_PATTERN.find_iter(&item.text).nth(location.string()).and_then(|line_match|
                line_match.start().checked_add(location.shift())
            ).ok_or(Error::Crash(Crash::Unexpected))
        } else {
            Ok(0)
        }
    }

    /// Edits `selection` within the `Document` to contain `text`.
    fn edit(&mut self, selection: &mut OldSelection, text: char) -> Result<(), Error> {
        let index = self.index_of(&selection.head)?;

        if let Some(item) = &mut self.item {
            match text {
                CHAR_BACKSPACE => {
                    if selection.starts_at_first_shift() && !selection.starts_at_first_string() {
                        selection.expand_up();
                        selection.move_head_to_end();
                        self.string_count -= 1;
                    }

                    // TODO: For now, do not care to check what is removed. But this may become important for
                    // multi-byte characters.
                    match item.text.remove(index) {
                        _ => {}
                    }
                }
                CHAR_ENTER => {
                    self.string_count += 1;
                }
                _ => {
                    item.text.insert(index, text);
                    selection.move_right();
                }
            }
        }

        Ok(())
    }

    /// Returns an enumerated iterator of the strings in the `Document`.
    ///
    /// The enumerated value starts at 1.
    fn strings(&self) -> impl Iterator<Item = (usize, &str)> {
        (1..).zip(if let Some(item) = &self.item {
            item.text.lines()
        } else {
            "".lines()
        })
    }

    /// Returns the line that is at `index`.
    ///
    /// [`None`] indicates no line exists at `index`.
    fn line(&self, index: usize) -> Option<&str> {
        if let Some(item) = &self.item {
            item.text.lines().nth(index)
        } else {
            None
        }
    }

    /// Returns the number of strings in the `Document`.
    const fn string_count(&self) -> usize {
        self.string_count
    }
}

/// The control panel of a [`Pane`].
#[derive(Clone, Debug)]
pub(crate) struct ControlPanel {
    /// The current output of the `ControlPanel`.
    ability_buffer: String,
    /// The index of the row where `ControlPanel` is output.
    row: usize,
    /// The current [`Ability`] displayed in the `ControlPanel`.
    ability: Ability,
}

impl ControlPanel {
    /// Creates a new `ControlPanel`.
    fn new(row: usize) -> Self {
        Self {
            ability_buffer: String::default(),
            ability: Ability::default(),
            row,
        }
    }

    /// Returns the changes to output the `ControlPanel`.
    fn changes(&self) -> Vec<Change> {
        // TODO: Could potentially improve to change only the chars that have been changed.
        vec![Change::Write{
            span: Span::entire_row(self.row),
            text: self.ability_buffer.clone(),
        }]
    }

    /// Clears the ability buffer.
    fn clear(&mut self) {
        self.ability_buffer.clear();
    }

    /// Adds a character.
    fn write(&mut self, input: char) -> Result<(), Error> {
        if input == CHAR_BACKSPACE {
            if self.ability_buffer.pop().is_none() {
                // This should never occur since we return Issue::EmptyAbility when ability_buffer
                // is empty, which switches to Show Mode.
                return Err(Error::Crash(Crash::Unexpected));
            } else if self.ability_buffer.is_empty() {
                return Err(Error::Issue(Issue::EmptyAbility));
            } else {
            }
        } else {
            self.ability_buffer.push(input);
        }

        self.ability = self.ability_buffer.parse::<Ability>()?;
        Ok(())
    }
}

impl Deref for ControlPanel {
    type Target = str;

    fn deref(&self) -> &str {
        self.ability_buffer.deref()
    }
}

/// An Error that occurs during a [`Sheet`] function.
#[derive(Debug)]
pub(crate) enum Error {
    /// An Error that can be resolved.
    Issue(Issue),
    /// An Error that is unable to be resolved.
    Crash(Crash),
}

/// An error that can be resolved by the application.
#[derive(Clone, Copy, Debug)]
pub enum Issue {
    /// Error occurred while converting types.
    Conversion,
    /// Error occurred while parsing.
    Parse,
    /// Attempted to remove element from an empty list.
    RemoveFromEmpty,
    /// Ability is empty.
    EmptyAbility,
    /// Path is invalid.
    InvalidPath,
    /// Ability is invalid.
    InvalidAbility,
}

/// An error that is unable to be resolved.
#[derive(Debug)]
pub enum Crash {
    /// An error from the [`io`].
    Io(io::Error),
    /// An error from the [`ui`].
    Ui(ui::Error),
    /// An unexpected error.
    Unexpected,
}

impl From<ui::Error> for Crash {
    fn from(other: ui::Error) -> Self {
        Crash::Ui(other)
    }
}

impl Display for Error {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::Issue(issue) => write!(f, "Issue: {}", match issue {
                Issue::Conversion => "Conversion",
                Issue::Parse => "Parse",
                Issue::RemoveFromEmpty => "Attempting to remove element from an empty list",
                Issue::EmptyAbility => "Ability is empty",
                Issue::InvalidPath => "Provided path is invalid",
                Issue::InvalidAbility => "Provided ability is invalid",
            }),
            Error::Crash(crash) => write!(f, "Crash: {}", match crash {
                Crash::Io(error) => error.to_string(),
                Crash::Ui(error) => error.to_string(),
                Crash::Unexpected => "Unexpected".to_string(),
            }),
        }
    }
}

impl From<ParseAbilityError> for Error {
    fn from(_other: ParseAbilityError) -> Self {
        Error::Issue(Issue::Parse)
    }
}

impl From<try_from::Void> for Error {
    fn from(_other: try_from::Void) -> Self {
        Error::Issue(Issue::Conversion)
    }
}

impl From<ui::Error> for Error {
    fn from(other: ui::Error) -> Self {
        Error::Crash(Crash::from(other))
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Error::Crash(Crash::Io(error))
    }
}
