//! The main window: creation, localised texts, the clock, the page callbacks,
//! the launchpad actions, the start-up centering and the event loop.
//!
//! Every step that changes what the user sees or what is stored on disk writes
//! one log line, so a log file alone is enough to reconstruct a session.
//!
//! Ownership of the state: the window properties are what the UI draws, and the
//! `Config` inside the `Rc<RefCell<..>>` below is what survives a restart. Every
//! handler mutates the configuration first, saves it, and only then pushes the
//! new state into the window, so the UI and the file can never disagree.

use crate::config;
use crate::i18n::i18n::Locale;
use crate::launchpad;
use crate::logs;
use crate::windows::dynamic_island;
use crate::windows::window_controls;
use crate::workspace;
use crate::VERSION;
use chrono::Local;
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, VecModel};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

// Generates the Rust bindings for MainWindow.slint (MainWindow, CallbackBridge,
// LogBridge, LaunchpadBridge, NavigationBridge, ...).
slint::include_modules!();

/// Log location shared by every line of this module.
const LOG_LOCATION: &str = "windows::main_window";

/// Duration of the page slide in the UI (see page-strip in MainWindow.slint).
/// The page that slides out is kept alive for this long plus a small margin,
/// so the animation always has something to show.
const PAGE_SLIDE_MS: u64 = 300;

/// Margin added to PAGE_SLIDE_MS before an inactive page is dropped.
const PAGE_DROP_MARGIN_MS: u64 = 80;

/// Configuration file, relative to the working directory of the process.
const CONFIG_PATH: &str = "config.json";

/// Tiles per grid row. Must match the `columns` property of LaunchpadPage.slint.
const LAUNCHPAD_COLUMNS: usize = 5;

/// Maximum number of island notices a single "add" action may post.
const MAX_ADD_NOTICES: usize = 3;

/// How long the start-up centering waits for the window to be laid out.
/// 16ms per poll is one frame, 120 polls are roughly two seconds.
const CENTER_POLL_INTERVAL: u64 = 16;
const CENTER_MAX_ATTEMPTS: u32 = 120;

/// Writes the current local time into the dynamic island chip.
fn update_time(app: &MainWindow) {
    let now = Local::now();
    let time_str = now.format("%Y-%m-%d %H:%M:%S").to_string();
    app.set_dynamic_island_text(time_str.into());
}

/// Applies every localised text of the window.
///
/// Called once at start-up and again after the language has been changed, so
/// there is exactly one place that knows which property holds which key.
fn update_app_texts(app: &MainWindow, locale: &Locale) {
    app.set_workspace(locale.t("workspace").into());
    app.set_launchpad(locale.t("launchpad").into());
    app.set_settings(locale.t("settings").into());
    app.set_general(locale.t("general").into());
    app.set_about(locale.t("about").into());
    app.set_language(locale.t("language").into());

    // Launchpad texts: the tile hint, the two toolbar buttons and the title of
    // the continuous launch dialog.
    app.set_launchpad_add_hint(locale.t("launchpad-add-hint").into());
    app.set_launchpad_run(locale.t("launchpad-continuous-run").into());
    app.set_launchpad_select(locale.t("launchpad-continuous-select").into());
    app.set_launchpad_selection_title(locale.t("launchpad-continuous-title").into());
    app.set_launchpad_done(locale.t("launchpad-done").into());

    // Entries of the tile context menu (right click on a tile).
    app.set_launchpad_menu_open(locale.t("menu-open").into());
    app.set_launchpad_menu_remove(locale.t("menu-remove").into());

    // Section label and button label of the general settings page.
    app.set_logs_text(locale.t("logs").into());
    app.set_open_logs_text(locale.t("open-logs").into());
    app.set_clear_logs_text(locale.t("clear-logs").into());

    // Workspace page: every label of the two panels.
    app.set_workspace_set_directory(locale.t("workspace-set-directory").into());
    app.set_workspace_add_directory(locale.t("workspace-add-directory").into());
    app.set_workspace_scope_caption(locale.t("workspace-scope-caption").into());
    app.set_workspace_scope_global(locale.t("workspace-scope-global").into());
    app.set_workspace_scope_local(locale.t("workspace-scope-local").into());
    app.set_workspace_tree_label(locale.t("workspace-tree-label").into());
    app.set_workspace_tree_empty(locale.t("workspace-tree-empty").into());
    app.set_workspace_directory_placeholder(locale.t("workspace-directory-placeholder").into());
    app.set_workspace_directory_caption(locale.t("workspace-directory-caption").into());
    app.set_workspace_project_caption(locale.t("workspace-project-caption").into());
    app.set_workspace_add_label(locale.t("workspace-add-label").into());
    app.set_workspace_add_hint(locale.t("workspace-add-hint").into());
    app.set_workspace_add_files(locale.t("workspace-add-files").into());
    app.set_workspace_add_folder(locale.t("workspace-add-folder").into());
    app.set_workspace_clear(locale.t("workspace-clear").into());
    app.set_workspace_added_empty(locale.t("workspace-added-empty").into());
    app.set_workspace_project_placeholder(locale.t("workspace-project-placeholder").into());
    app.set_workspace_add_project(locale.t("workspace-add-project").into());
    app.set_workspace_template_label(locale.t("workspace-template-label").into());
    app.set_workspace_import_folder(locale.t("workspace-import-folder").into());
    app.set_workspace_default_template(locale.t("workspace-default-template").into());
    app.set_workspace_kind_ext(locale.t("workspace-kind-ext").into());
    app.set_workspace_kind_keyword(locale.t("workspace-kind-keyword").into());
    app.set_workspace_other_label(locale.t("workspace-other-label").into());
    app.set_workspace_other_hint(locale.t("workspace-other-hint").into());
    app.set_workspace_other_clear(locale.t("workspace-other-clear").into());
    app.set_workspace_rules_label(locale.t("workspace-rules-label").into());
    app.set_workspace_add_rule(locale.t("workspace-add-rule").into());
    app.set_workspace_rules_hint(locale.t("workspace-rules-hint").into());
    app.set_workspace_rules_empty(locale.t("workspace-rules-empty").into());
    app.set_workspace_rule_choose(locale.t("workspace-rule-choose").into());
    app.set_workspace_rule_remove(locale.t("workspace-rule-remove").into());
    app.set_workspace_rule_target_placeholder(locale.t("workspace-rule-target").into());
    app.set_workspace_value_ext(locale.t("workspace-value-ext").into());
    app.set_workspace_value_keyword(locale.t("workspace-value-keyword").into());
    app.set_workspace_process(locale.t("workspace-process").into());
}

/// Builds the localised text of one notice.
///
/// Every notice key contains a `{}` placeholder where the entry name or the
/// number of entries is inserted; keeping that rule in one helper means the
/// call sites only pass the value.
fn notice_text(locale: &Locale, key: &str, value: &str) -> String {
    locale.t(key).replace("{}", value)
}

/// Writes the configuration to disk and logs why it was written.
///
/// The reason is part of the log line on purpose: with one line per user action
/// it is otherwise impossible to tell whether a save was caused by a page
/// switch, a new tile or the shutdown.
fn save_config(config: &Rc<RefCell<config::Config>>, reason: &str) {
    let snapshot = config.borrow().clone();
    match snapshot.save(CONFIG_PATH) {
        Ok(()) => logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Configuration written ({}).", reason),
            "info",
        ),
        Err(error) => logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Configuration could not be written ({}): {}.", reason, error),
            "err",
        ),
    }
}

/// Pushes the launchpad content of `config` into the window.
///
/// Three parallel arrays are used instead of a model of structs: a struct would
/// have to be declared in MainWindow.slint and passed down to the page, while
/// plain `[string]`/`[bool]` properties work across the import boundary.
fn refresh_launchpad(app: &MainWindow, config: &config::Config, locale: &Locale) {
    let items = &config.launchpad.items;
    let continuous = &config.launchpad.continuous;

    let names: Vec<SharedString> = items
        .iter()
        .map(|item| SharedString::from(item.name.as_str()))
        .collect();
    let letters: Vec<SharedString> = items
        .iter()
        .map(|item| SharedString::from(launchpad::letter(&item.name)))
        .collect();
    let selected: Vec<bool> = items
        .iter()
        .map(|item| launchpad::is_continuous(continuous, item))
        .collect();

    app.set_launchpad_item_names(ModelRc::new(VecModel::from(names)));
    app.set_launchpad_item_letters(ModelRc::new(VecModel::from(letters)));
    app.set_launchpad_item_continuous(ModelRc::new(VecModel::from(selected)));

    // The "+" tile needs a slot as well, so the row count is rounded up over
    // one extra slot. Slint cannot do that division, hence it happens here.
    let slots = items.len() + 1;
    let rows = slots.div_ceil(LAUNCHPAD_COLUMNS).max(1);
    app.set_launchpad_row_count(i32::try_from(rows).unwrap_or(i32::MAX));

    let summary = launchpad::continuous_summary(
        items,
        continuous,
        &locale.t("launchpad-continuous-none"),
    );
    app.set_launchpad_summary(summary.as_str().into());

    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!(
            "Launchpad refreshed: {} tile(s), {} selected for a continuous launch (box shows \"{}\").",
            items.len(),
            continuous.len(),
            summary
        ),
        "info",
    );
}

/// Adds every path of `paths` to the launchpad, saves the file and refreshes
/// the UI once. Duplicates and unreadable paths are logged and skipped.
///
/// Each added/skipped path also posts a notice to the island, but at most
/// [`MAX_ADD_NOTICES`] of them: a multi selection of twenty files must not keep
/// the clock hidden for a whole minute.
fn add_paths(
    app: &MainWindow,
    config: &Rc<RefCell<config::Config>>,
    locale: &Locale,
    island: &dynamic_island::IslandRef,
    paths: &[String],
) {
    let mut added = 0usize;
    let mut notices = 0usize;
    {
        let mut guard = config.borrow_mut();
        let launchpad_state = &mut guard.launchpad;
        for path in paths {
            // Name shown in the notice: the file stem for files, the folder name
            // for folders (the same rule the tiles use).
            let name = launchpad::entry_name(Path::new(path));

            match launchpad::add_item(&mut launchpad_state.items, Path::new(path)) {
                launchpad::AddOutcome::Added(index) => {
                    added += 1;
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Added tile #{} for \"{}\".", index, path),
                        "info",
                    );
                    if notices < MAX_ADD_NOTICES {
                        dynamic_island::notify(
                            island,
                            notice_text(locale, "notice-added", &name),
                            false,
                        );
                        notices += 1;
                    }
                }
                launchpad::AddOutcome::Duplicate => {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Skipped \"{}\": already on the launchpad.", path),
                        "info",
                    );
                    if notices < MAX_ADD_NOTICES {
                        dynamic_island::notify(
                            island,
                            notice_text(locale, "notice-duplicate", &name),
                            true,
                        );
                        notices += 1;
                    }
                }
                launchpad::AddOutcome::Invalid => {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Skipped \"{}\": no such file or folder.", path),
                        "warn",
                    );
                    if notices < MAX_ADD_NOTICES {
                        dynamic_island::notify(
                            island,
                            notice_text(locale, "notice-missing", &name),
                            true,
                        );
                        notices += 1;
                    }
                }
            }
        }
    }

    if added == 0 {
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "No tile was added, the configuration stays unchanged.",
            "info",
        );
        return;
    }

    save_config(config, &format!("{} tile(s) added", added));
    refresh_launchpad(app, &config.borrow(), locale);
}

/// State of the workspace page that only lives while the window is open.
///
/// Nothing here is persisted: the working directory, the template and the rules
/// belong to `config.json`, while the expanded folders and the staged files are
/// session state — restoring a half staged list after a restart would surprise
/// more than it helps.
struct WorkspaceUi {
    /// Folders whose children are currently visible in the tree.
    expanded: std::collections::HashSet<std::path::PathBuf>,
    /// Files and folders staged for the classifier.
    added: Vec<String>,
    /// The rule kinds the page is showing. A kind change patches one row of
    /// this model instead of rebuilding it, so the sliding highlight can
    /// animate: a rebuilt row would start at its new position.
    kinds: RefCell<Option<Rc<VecModel<SharedString>>>>,
    /// Scope selected for the next rule that is added ("global" / "local").
    /// Session state on purpose: the last choice should not be inherited from a
    /// completely different session.
    scope: String,
}

/// Shared handle of the workspace session state.
type WorkspaceRef = Rc<RefCell<WorkspaceUi>>;

/// Shortens a long path for a narrow box, keeping the end.
///
/// The end of a path is the part that differs between two rules, so the front
/// is the half worth dropping. The full path stays in the configuration and in
/// the log, the box is display only.
fn elide_front(path: &str, keep: usize) -> String {
    let parts: Vec<&str> = path
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() <= keep {
        return path.to_string();
    }
    format!("…\\{}", parts[parts.len() - keep..].join("\\"))
}

/// True when `rule` belongs to the working directory `directory`.
///
/// Global rules are shown everywhere; a local rule only in the directory it was
/// created in. A local rule without a directory can no longer be seen, which is
/// why `Config::normalize_workspace` turns those into global rules.
fn rule_visible(rule: &config::ClassifyRule, directory: &str) -> bool {
    if rule.scope != config::RULE_SCOPE_LOCAL {
        return true;
    }
    !rule.directory.trim().is_empty() && rule.directory.eq_ignore_ascii_case(directory)
}

/// Indices inside `config.workspace.rules` that the current directory shows.
///
/// The list keeps the stored order, so removing one row cannot reorder the rest.
fn visible_rule_indices(config: &config::Config) -> Vec<usize> {
    let directory = config.workspace.directory.as_str();
    config
        .workspace
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| rule_visible(rule, directory))
        .map(|(index, _)| index)
        .collect()
}

/// The rules the classifier applies in the current working directory.
///
/// Hidden rules of other directories must not match, so the classifier receives
/// the filtered list instead of the whole configuration.
fn rules_for_directory(config: &config::Config) -> Vec<config::ClassifyRule> {
    config
        .workspace
        .rules
        .iter()
        .filter(|rule| rule_visible(rule, &config.workspace.directory))
        .cloned()
        .collect()
}

/// Maps the index the page uses (which only counts ordinary rules) to the index
/// inside `config.workspace.rules` (which also holds the fallback rule and the
/// rules of the other working directories).
///
/// The page shows the fallback rule in its own block instead of as a list row,
/// so the two indices drift apart as soon as one exists.
fn ui_rule_index(config: &config::Config, ui_index: usize) -> Option<usize> {
    visible_rule_indices(config)
        .into_iter()
        .filter(|index| config.workspace.rules[*index].kind != "other")
        .nth(ui_index)
}

/// Text shown under the template heading: the configured template folder, or
/// "built-in template" when the built-in structure is used.
fn template_text(config: &config::Config, locale: &Locale) -> String {
    let custom = config.workspace.template.trim();
    if custom.is_empty() {
        return locale.t("workspace-template-builtin").to_string();
    }
    if std::path::Path::new(custom).is_dir() {
        return custom.to_string();
    }
    format!("{} ({})", custom, locale.t("workspace-template-missing"))
}

/// Pushes the whole workspace state into the window properties.
///
/// Slint has no struct models that Rust can fill, so every list is sent as a
/// flat array and the page indents the rows itself (see WorkspacePage.slint).
fn refresh_workspace(
    app: &MainWindow,
    config: &config::Config,
    locale: &Locale,
    ui: &WorkspaceUi,
) {
    app.set_workspace_directory(config.workspace.directory.clone().into());

    // Drop-down of the known working directories. The label keeps the end of
    // the path, because that is the part that tells two directories apart.
    let labels: Vec<SharedString> = config
        .workspace
        .directories
        .iter()
        .map(|directory| SharedString::from(elide_front(directory, 2)))
        .collect();
    let active_index = config
        .workspace
        .directories
        .iter()
        .position(|directory| {
            directory.eq_ignore_ascii_case(config.workspace.directory.as_str())
        })
        .unwrap_or(0);
    app.set_workspace_directories(ModelRc::new(VecModel::from(labels)));
    app.set_workspace_directory_index(i32::try_from(active_index).unwrap_or(0));

    let rows = match workspace::usable_directory(&config.workspace.directory) {
        Some(root) => workspace::flatten(&root, &ui.expanded),
        None => Vec::new(),
    };
    let mut depths: Vec<i32> = Vec::with_capacity(rows.len());
    let mut names: Vec<SharedString> = Vec::with_capacity(rows.len());
    let mut paths: Vec<SharedString> = Vec::with_capacity(rows.len());
    let mut dirs: Vec<bool> = Vec::with_capacity(rows.len());
    let mut expanded: Vec<bool> = Vec::with_capacity(rows.len());
    let mut children: Vec<bool> = Vec::with_capacity(rows.len());
    for row in &rows {
        depths.push(row.depth);
        names.push(SharedString::from(row.name.clone()));
        paths.push(SharedString::from(row.path.clone()));
        dirs.push(row.is_dir);
        expanded.push(row.expanded);
        children.push(row.has_children);
    }
    app.set_workspace_tree_depths(ModelRc::new(VecModel::from(depths)));
    app.set_workspace_tree_names(ModelRc::new(VecModel::from(names)));
    app.set_workspace_tree_paths(ModelRc::new(VecModel::from(paths)));
    app.set_workspace_tree_is_dirs(ModelRc::new(VecModel::from(dirs)));
    app.set_workspace_tree_expanded(ModelRc::new(VecModel::from(expanded)));
    app.set_workspace_tree_has_children(ModelRc::new(VecModel::from(children)));

    let added: Vec<SharedString> = ui
        .added
        .iter()
        .map(|path| SharedString::from(path.clone()))
        .collect();
    app.set_workspace_added(ModelRc::new(VecModel::from(added)));

    app.set_workspace_template_current(template_text(config, locale).into());

    // Only the rules of the current directory are shown: every global rule plus
    // the local rules that were created for this directory. Rules of another
    // directory stay in the configuration but are not part of the page.
    let visible = visible_rule_indices(config);
    let hidden = config.workspace.rules.len().saturating_sub(visible.len());
    let mut kinds: Vec<SharedString> = Vec::with_capacity(visible.len());
    let mut values: Vec<SharedString> = Vec::with_capacity(visible.len());
    let mut targets: Vec<SharedString> = Vec::with_capacity(visible.len());
    let mut scopes: Vec<SharedString> = Vec::with_capacity(visible.len());
    let mut other_target = String::new();
    for index in visible {
        let rule = &config.workspace.rules[index];
        // The fallback rule is not a row of the list: the page shows it in its
        // own block below the list, so it is pulled out here.
        if rule.kind == "other" {
            other_target = elide_front(&rule.target, 3);
            continue;
        }
        // The page compares against the id so the active segment can be
        // highlighted; the captions are set once in update_app_texts.
        kinds.push(SharedString::from(rule.kind.clone()));
        values.push(SharedString::from(rule.value.clone()));
        targets.push(SharedString::from(elide_front(&rule.target, 2)));
        scopes.push(SharedString::from(rule.scope.clone()));
    }
    let kinds_model = Rc::new(VecModel::from(kinds));
    *ui.kinds.borrow_mut() = Some(kinds_model.clone());
    app.set_workspace_rule_kinds(ModelRc::from(kinds_model));
    app.set_workspace_rule_values(ModelRc::new(VecModel::from(values)));
    app.set_workspace_rule_targets(ModelRc::new(VecModel::from(targets)));
    app.set_workspace_rule_scopes(ModelRc::new(VecModel::from(scopes)));
    app.set_workspace_rule_scope(ui.scope.clone().into());
    app.set_workspace_other_target(other_target.into());

    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!(
            "Workspace refreshed: {} tree row(s), {} staged path(s), {} of {} rule(s) shown for \"{}\", new-rule scope \"{}\", template \"{}\".",
            rows.len(),
            ui.added.len(),
            config.workspace.rules.len() - hidden,
            config.workspace.rules.len(),
            config.workspace.directory,
            ui.scope,
            config.workspace.template
        ),
        "info",
    );
}

/// Creates the main window and runs the event loop until it is closed.
pub fn main_window() -> Result<(), Box<dyn std::error::Error>> {
    logs::logs_output::logs_output(LOG_LOCATION, "MainWindow starting.", "info");

    // The configuration is read before the window exists so that the first
    // paint already shows the stored language and page.
    let config = Rc::new(RefCell::new(config::Config::load_or_create(CONFIG_PATH)?));
    {
        let loaded = config.borrow();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "Configuration ready: language = \"{}\", page = {}/{}, {} launchpad tile(s).",
                loaded.language,
                loaded.page.main,
                loaded.page.settings,
                loaded.launchpad.items.len()
            ),
            "info",
        );
    }

    let app = MainWindow::new().expect("Failed to create the main window");
    logs::logs_output::logs_output(LOG_LOCATION, "Main window created (1200x710).", "info");

    // Queue of the dynamic island. `update_time` is handed over as the "island is
    // idle again" callback, so the clock comes back by itself once the last
    // notice disappeared (see src/windows/dynamic_island.rs).
    let island = dynamic_island::new(&app, update_time);

    // Version shown in the About page.
    app.set_version(VERSION.into());
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("Version label set to \"{}\".", VERSION),
        "info",
    );

    // Log every action the UI reports through the shared LogBridge (page
    // switches, settings rows, ...). One line per user action.
    app.global::<LogBridge>().on_log(|event| {
        logs::logs_output::logs_output("ui::event", event.as_str(), "info");
    });

    // --- Custom title bar -------------------------------------------------
    // The window has no native frame, so dragging, minimizing and closing are
    // forwarded to the Win32 commands in window_controls. The drag handler needs
    // the window itself: the system move loop swallows the mouse release, and
    // the release has to be handed back to the UI (see `begin_drag`).
    let drag_app = app.as_weak();
    app.global::<WindowBridge>().on_start_drag(move || {
        if let Some(app) = drag_app.upgrade() {
            window_controls::begin_drag(&app);
        }
    });
    app.global::<WindowBridge>().on_minimize(|| {
        window_controls::minimize();
    });
    app.global::<WindowBridge>().on_close(|| {
        window_controls::close();
    });

    // Clock of the dynamic island: update once immediately, then every second.
    // While a notice is on screen the clock stays out of the way, otherwise the
    // two would overwrite each other once per second.
    update_time(&app);
    let clock_handle = app.as_weak();
    let clock_island = island.clone();
    let clock_timer = Timer::default();
    clock_timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_secs(1),
        move || {
            if dynamic_island::is_busy(&clock_island) {
                return;
            }
            if let Some(app) = clock_handle.upgrade() {
                update_time(&app);
            }
        },
    );
    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Dynamic island clock started (updates every 1000ms).",
        "info",
    );

    // Apply the stored language to every text of the window. The locale is kept
    // behind a `RefCell` because the launchpad summary needs it again whenever
    // the tile list changes.
    let stored_language = config.borrow().language.clone();
    let locale = Rc::new(RefCell::new(Locale::load(&stored_language)?));
    update_app_texts(&app, &locale.borrow());
    app.set_current_language(stored_language.clone().into());
    logs::logs_output::logs_output(
        LOG_LOCATION,
        &format!("UI texts applied for language \"{}\".", stored_language),
        "info",
    );

    // Restore the page the user was on when the application was closed. The
    // switch derives its highlight from `operation-area-page`, so setting it
    // here also moves the capsule highlight to the right segment.
    {
        let loaded = config.borrow();
        let page = loaded.page.main.clamp(0, 2);
        app.set_operation_area_page(page);
        app.set_settings_page(loaded.page.settings);
        // Only the restored page is created; the other two stay out of memory
        // until the user actually switches to them (see `on_page_changed`).
        app.set_page_alive_workspace(page == 0);
        app.set_page_alive_launchpad(page == 1);
        app.set_page_alive_settings(page == 2);
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "Restored page {}/{} (0 = workspace, 1 = launchpad, 2 = settings); only that page is built.",
                page, loaded.page.settings
            ),
            "info",
        );
    }

    // First fill of the launchpad tiles.
    refresh_launchpad(&app, &config.borrow(), &locale.borrow());

    // Session state of the workspace page. It is created before the language
    // handler because a language change has to rebuild the rule captions too.
    let workspace_ui: WorkspaceRef = Rc::new(RefCell::new(WorkspaceUi {
        expanded: std::collections::HashSet::new(),
        added: Vec::new(),
        kinds: RefCell::new(None),
        scope: config::RULE_SCOPE_GLOBAL.to_string(),
    }));

    // --- Language selection coming from the general settings page ----------
    let language_handle = app.as_weak();
    let language_config = config.clone();
    let language_locale = locale.clone();
    let language_workspace = workspace_ui.clone();
    app.global::<CallbackBridge>()
        .on_general_page_language_selected(move |value| {
            let Some(app) = language_handle.upgrade() else {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    "Language changed after the window was destroyed, ignored.",
                    "warn",
                );
                return;
            };

            let previous = language_config.borrow().language.clone();
            if previous.as_str() == value.as_str() {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Language selection repeated (\"{}\"), nothing to do.", value),
                    "info",
                );
                return;
            }

            // Load first: when the new locale cannot be read the current one
            // stays active and nothing is written to disk.
            match Locale::load(&value) {
                Ok(loaded) => {
                    language_config.borrow_mut().language = value.to_string();
                    save_config(&language_config, "language change");
                    *language_locale.borrow_mut() = loaded;

                    let locale = language_locale.borrow();
                    update_app_texts(&app, &locale);
                    // The summary box is built from the locale, so it has to be
                    // rebuilt together with the other texts.
                    refresh_launchpad(&app, &language_config.borrow(), &locale);
                    // Rule captions are localised as well.
                    refresh_workspace(
                        &app,
                        &language_config.borrow(),
                        &locale,
                        &language_workspace.borrow(),
                    );
                    app.set_current_language(value.clone());
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Language changed from \"{}\" to \"{}\".", previous, value),
                        "info",
                    );
                }
                Err(error) => {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Language \"{}\" could not be loaded: {}.", value, error),
                        "err",
                    );
                }
            }
        });

    // --- General settings: open the folder that holds the log files -------
    // One click, one log line, one Explorer window: the folder is resolved to an
    // absolute path first because Explorer resolves relative paths against its
    // own working directory.
    app.global::<CallbackBridge>().on_general_page_open_logs(|| {
        let directory = logs::logs_path::log_directory();
        let path = directory.to_string_lossy().to_string();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Opening the log folder \"{}\".", path),
            "info",
        );
        match launchpad::launcher::open(&path) {
            Ok(()) => logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("Log folder opened (\"{}\").", path),
                "info",
            ),
            Err(error) => logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("The log folder could not be opened: {}.", error),
                "err",
            ),
        }
    });

    // --- General settings: clear every log of an older run ----------------
    // The file of this run is still open and must not be deleted while it is
    // being written to, so `clear_old_logs` keeps it by name.
    let clear_locale = locale.clone();
    let clear_island = island.clone();
    app.global::<CallbackBridge>()
        .on_general_page_clear_logs(move || {
            let locale = clear_locale.borrow();
            match logs::logs_path::clear_old_logs() {
                Ok(0) => dynamic_island::notify(
                    &clear_island,
                    locale.t("notice-logs-cleared-none").to_string(),
                    false,
                ),
                Ok(removed) => dynamic_island::notify(
                    &clear_island,
                    notice_text(&locale, "notice-logs-cleared", &removed.to_string()),
                    false,
                ),
                Err(error) => {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Clearing the old logs failed: {}.", error),
                        "err",
                    );
                    dynamic_island::notify(
                        &clear_island,
                        locale.t("notice-logs-clear-failed").to_string(),
                        true,
                    );
                }
            }
        });

    // --- Launchpad: start one tile ----------------------------------------
    let launch_handle = app.as_weak();
    let launch_config = config.clone();
    let launch_locale = locale.clone();
    let launch_island = island.clone();
    app.global::<LaunchpadBridge>().on_launch(move |index| {
        if launch_handle.upgrade().is_none() {
            return;
        }
        let entry = launch_config
            .borrow()
            .launchpad
            .items
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .cloned();

        match entry {
            Some(item) => {
                let result = launchpad::launcher::open(&item.path);
                launchpad::launcher::log_outcome(&item.name, &item.path, &result);

                // The island reports the outcome: black for success, red for a
                // failed start (the log keeps the raw error code).
                let locale = launch_locale.borrow();
                match result {
                    Ok(()) => dynamic_island::notify(
                        &launch_island,
                        notice_text(&locale, "notice-launched", &item.name),
                        false,
                    ),
                    Err(_) => dynamic_island::notify(
                        &launch_island,
                        notice_text(&locale, "notice-launch-failed", &item.name),
                        true,
                    ),
                }
            }
            None => {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Launch requested for missing tile #{}", index),
                    "err",
                );
                let locale = launch_locale.borrow();
                dynamic_island::notify(
                    &launch_island,
                    notice_text(&locale, "notice-missing", &index.to_string()),
                    true,
                );
            }
        }
    });

    // --- Launchpad: remove one tile ---------------------------------------
    let remove_handle = app.as_weak();
    let remove_config = config.clone();
    let remove_locale = locale.clone();
    let remove_island = island.clone();
    app.global::<LaunchpadBridge>().on_remove(move |index| {
        let Some(app) = remove_handle.upgrade() else {
            return;
        };

        let removed = {
            let mut guard = remove_config.borrow_mut();
            let launchpad_state = &mut guard.launchpad;
            launchpad::remove_item(
                &mut launchpad_state.items,
                &mut launchpad_state.continuous,
                usize::try_from(index).unwrap_or(usize::MAX),
            )
        };

        match removed {
            Some(item) => {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Removed tile \"{}\" ({}).", item.name, item.path),
                    "info",
                );
                save_config(&remove_config, "tile removed");
                refresh_launchpad(&app, &remove_config.borrow(), &remove_locale.borrow());
                dynamic_island::notify(
                    &remove_island,
                    notice_text(&remove_locale.borrow(), "notice-removed", &item.name),
                    false,
                );
            }
            None => logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("Remove requested for missing tile #{}", index),
                "err",
            ),
        }
    });

    // --- Launchpad: add files (multi selection) ---------------------------
    let add_files_handle = app.as_weak();
    let add_files_config = config.clone();
    let add_files_locale = locale.clone();
    let add_files_island = island.clone();
    app.global::<LaunchpadBridge>().on_add_items(move || {
        let Some(app) = add_files_handle.upgrade() else {
            return;
        };

        let title = add_files_locale.borrow().t("launchpad-add-file-title");
        let owner = crate::windows::window_center::main_window_handle();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Opening the file picker for the launchpad.",
            "info",
        );
        let paths = launchpad::dialog::pick_files(owner, &title);
        if paths.is_empty() {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "File picker closed without a selection.",
                "info",
            );
            return;
        }

        let locale = add_files_locale.borrow();
        add_paths(&app, &add_files_config, &locale, &add_files_island, &paths);
    });

    // --- Launchpad: add a folder ------------------------------------------
    let add_folder_handle = app.as_weak();
    let add_folder_config = config.clone();
    let add_folder_locale = locale.clone();
    let add_folder_island = island.clone();
    app.global::<LaunchpadBridge>().on_add_folder(move || {
        let Some(app) = add_folder_handle.upgrade() else {
            return;
        };

        let title = add_folder_locale.borrow().t("launchpad-add-folder-title");
        let owner = crate::windows::window_center::main_window_handle();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Opening the folder picker for the launchpad.",
            "info",
        );
        match launchpad::dialog::pick_folder(owner, &title) {
            Some(path) => {
                let locale = add_folder_locale.borrow();
                add_paths(&app, &add_folder_config, &locale, &add_folder_island, &[path]);
            }
            None => logs::logs_output::logs_output(
                LOG_LOCATION,
                "Folder picker closed without a selection.",
                "info",
            ),
        }
    });

    // --- Launchpad: toggle one entry of the continuous launch -------------
    let toggle_handle = app.as_weak();
    let toggle_config = config.clone();
    let toggle_locale = locale.clone();
    app.global::<LaunchpadBridge>()
        .on_toggle_continuous(move |index| {
            let Some(app) = toggle_handle.upgrade() else {
                return;
            };

            let new_state = {
                let mut guard = toggle_config.borrow_mut();
                let launchpad_state = &mut guard.launchpad;
                launchpad::toggle_continuous(
                    &launchpad_state.items,
                    &mut launchpad_state.continuous,
                    usize::try_from(index).unwrap_or(usize::MAX),
                )
            };

            match new_state {
                Some(selected) => {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!(
                            "Continuous launch selection of tile #{} is now {}.",
                            index,
                            if selected { "on" } else { "off" }
                        ),
                        "info",
                    );
                    save_config(&toggle_config, "continuous selection changed");
                    refresh_launchpad(&app, &toggle_config.borrow(), &toggle_locale.borrow());
                }
                None => logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Continuous launch toggle for missing tile #{}", index),
                    "err",
                ),
            }
        });

    // --- Launchpad: start every selected entry ----------------------------
    let run_handle = app.as_weak();
    let run_config = config.clone();
    let run_locale = locale.clone();
    let run_island = island.clone();
    app.global::<LaunchpadBridge>().on_run_continuous(move || {
        if run_handle.upgrade().is_none() {
            return;
        }

        // Names and paths are collected first so the configuration borrow is
        // released before processes are started (a start can take a moment).
        let entries: Vec<(String, String)> = {
            let guard = run_config.borrow();
            guard
                .launchpad
                .items
                .iter()
                .filter(|item| launchpad::is_continuous(&guard.launchpad.continuous, item))
                .map(|item| (item.name.clone(), item.path.clone()))
                .collect()
        };

        if entries.is_empty() {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Continuous launch requested, but no tile is selected.",
                "warn",
            );
            // The most common mistake, so it gets its own (red) notice instead
            // of failing silently.
            dynamic_island::notify(
                &run_island,
                run_locale.borrow().t("notice-no-continuous"),
                true,
            );
            return;
        }

        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Continuous launch starting {} entry/entries.", entries.len()),
            "info",
        );
        let total = entries.len();
        let mut failed = 0usize;
        for (name, path) in entries {
            let result = launchpad::launcher::open(&path);
            launchpad::launcher::log_outcome(&name, &path, &result);
            if result.is_err() {
                failed += 1;
            }
        }

        // One summary notice instead of one per entry: a continuous launch of ten
        // files would otherwise hide the clock for half a minute.
        let locale = run_locale.borrow();
        if failed == 0 {
            dynamic_island::notify(
                &run_island,
                notice_text(&locale, "notice-continuous-ok", &total.to_string()),
                false,
            );
        } else {
            dynamic_island::notify(
                &run_island,
                notice_text(&locale, "notice-continuous-failed", &failed.to_string()),
                true,
            );
        }
    });

    // --- Workspace page ---------------------------------------------------
    // Every handler below follows the same shape: validate the request, change
    // the configuration, save it with a reason, refresh the models and log one
    // line. The page itself never touches the filesystem.
    // The session state itself was created before the language handler.

    // First fill, so the page is complete before it is ever shown.
    refresh_workspace(
        &app,
        &config.borrow(),
        &locale.borrow(),
        &workspace_ui.borrow(),
    );

    // Working directory: pick a folder and scan it again from the root.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_add_directory(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let title = ws_locale.borrow().t("workspace-add-directory").to_string();
        let owner = crate::windows::window_center::main_window_handle();
        let Some(directory) = launchpad::dialog::pick_folder(owner, &title) else {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Choosing the working directory was cancelled.",
                "info",
            );
            return;
        };
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Working directory set to \"{}\".", directory),
            "info",
        );
        {
            let mut guard = ws_config.borrow_mut();
            // Adding a folder that is already known simply switches to it; the
            // list must not collect duplicates.
            if !guard
                .workspace
                .directories
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&directory))
            {
                guard.workspace.directories.push(directory.clone());
            }
            guard.workspace.directory = directory;
        }
        // The expanded paths belong to the old tree and mean nothing in the new
        // one, so the new directory starts fully collapsed.
        ws_state.borrow_mut().expanded.clear();
        save_config(&ws_config, "working directory changed");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Switch between the known working directories from the drop-down. The
    // rules list follows immediately, because local rules belong to exactly one
    // directory.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>()
        .on_directory_selected(move |index| {
            let Some(app) = ws_handle.upgrade() else {
                return;
            };
            let index = index.max(0) as usize;
            let selected = ws_config
                .borrow()
                .workspace
                .directories
                .get(index)
                .cloned();
            let Some(directory) = selected else {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Directory selection {} does not exist.", index),
                    "warn",
                );
                return;
            };
            if ws_config.borrow().workspace.directory.eq_ignore_ascii_case(&directory) {
                return;
            }
            logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("Switched the working directory to \"{}\".", directory),
                "info",
            );
            ws_config.borrow_mut().workspace.directory = directory;
            ws_state.borrow_mut().expanded.clear();
            save_config(&ws_config, "working directory switched");
            refresh_workspace(
                &app,
                &ws_config.borrow(),
                &ws_locale.borrow(),
                &ws_state.borrow(),
            );
        });

    // Expand / collapse one folder row of the tree.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_toggle_dir(move |path| {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let path = std::path::PathBuf::from(path.as_str());
        let expanded = {
            let mut state = ws_state.borrow_mut();
            if state.expanded.remove(&path) {
                false
            } else {
                state.expanded.insert(path.clone());
                true
            }
        };
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "Folder \"{}\" {}.",
                path.display(),
                if expanded { "expanded" } else { "collapsed" }
            ),
            "info",
        );
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Stage files for the classifier.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_add_files(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let title = ws_locale.borrow().t("workspace-add-files").to_string();
        let owner = crate::windows::window_center::main_window_handle();
        let files = launchpad::dialog::pick_files(owner, &title);
        if files.is_empty() {
            logs::logs_output::logs_output(LOG_LOCATION, "Adding files was cancelled.", "info");
            return;
        }
        {
            let mut state = ws_state.borrow_mut();
            for file in files {
                if !state.added.contains(&file) {
                    state.added.push(file);
                }
            }
        }
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Staged {} path(s) for classification.", ws_state.borrow().added.len()),
            "info",
        );
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Stage one folder for the classifier.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_add_folder(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let title = ws_locale.borrow().t("workspace-add-folder").to_string();
        let owner = crate::windows::window_center::main_window_handle();
        let Some(folder) = launchpad::dialog::pick_folder(owner, &title) else {
            logs::logs_output::logs_output(LOG_LOCATION, "Adding a folder was cancelled.", "info");
            return;
        };
        {
            let mut state = ws_state.borrow_mut();
            if !state.added.contains(&folder) {
                state.added.push(folder);
            }
        }
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Staged {} path(s) for classification.", ws_state.borrow().added.len()),
            "info",
        );
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Drop the staged list; the files themselves are never touched.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_clear_added(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let removed = ws_state.borrow_mut().added.len();
        ws_state.borrow_mut().added.clear();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Cleared the staged list ({} path(s)).", removed),
            "info",
        );
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Create a project folder from the typed name and the current template.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    let ws_island = island.clone();
    app.global::<WorkspaceBridge>().on_create_project(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let name = app.get_workspace_project_name().to_string();
        let (directory, template) = {
            let config = ws_config.borrow();
            (config.workspace.directory.clone(), config.workspace.template.clone())
        };
        let Some(workdir) = workspace::usable_directory(&directory) else {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Creating a project needs a working directory first.",
                "warn",
            );
            app.set_workspace_result(ws_locale.borrow().t("workspace-need-directory").into());
            app.set_workspace_result_error(true);
            dynamic_island::notify(
                &ws_island,
                ws_locale.borrow().t("workspace-need-directory").to_string(),
                true,
            );
            return;
        };
        match workspace::project::create(&workdir, &name, &template) {
            Ok(project) => {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Project created: {}.", project.display()),
                    "info",
                );
                app.set_workspace_result(
                    notice_text(&ws_locale.borrow(), "workspace-project-created", &project.display().to_string()).into(),
                );
                app.set_workspace_result_error(false);
                app.set_workspace_project_name("".into());
                dynamic_island::notify(
                    &ws_island,
                    notice_text(&ws_locale.borrow(), "workspace-project-created", &project.display().to_string()),
                    false,
                );
                refresh_workspace(
                    &app,
                    &ws_config.borrow(),
                    &ws_locale.borrow(),
                    &ws_state.borrow(),
                );
            }
            Err(message) => {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Creating a project failed: {}.", message),
                    "err",
                );
                app.set_workspace_result(message.clone().into());
                app.set_workspace_result_error(true);
                dynamic_island::notify(&ws_island, message, true);
            }
        }
    });

    // Template: a folder, a zip, or back to the default one.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_template_folder(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let title = ws_locale.borrow().t("workspace-import-folder").to_string();
        let owner = crate::windows::window_center::main_window_handle();
        let Some(folder) = launchpad::dialog::pick_folder(owner, &title) else {
            logs::logs_output::logs_output(LOG_LOCATION, "Choosing a template folder was cancelled.", "info");
            return;
        };
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Template set to the folder \"{}\".", folder),
            "info",
        );
        ws_config.borrow_mut().workspace.template = folder;
        save_config(&ws_config, "template changed");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_template_default(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        ws_config.borrow_mut().workspace.template = String::new();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            "Template reset to the built-in structure.",
            "info",
        );
        save_config(&ws_config, "template reset to default");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Rules: add, flip the kind, edit the value, choose the target, remove.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    let ws_island = island.clone();
    app.global::<WorkspaceBridge>().on_add_rule(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let scope = ws_state.borrow().scope.clone();
        let directory = ws_config.borrow().workspace.directory.clone();
        if scope == config::RULE_SCOPE_LOCAL && directory.trim().is_empty() {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "A local rule cannot be added before a working directory is set.",
                "warn",
            );
            dynamic_island::notify(
                &ws_island,
                ws_locale.borrow().t("workspace-need-directory").to_string(),
                true,
            );
            return;
        }
        {
            let mut guard = ws_config.borrow_mut();
            let local = scope == config::RULE_SCOPE_LOCAL;
            guard.workspace.rules.push(config::ClassifyRule {
                kind: "ext".to_string(),
                value: String::new(),
                target: String::new(),
                scope: scope.clone(),
                directory: if local { directory } else { String::new() },
            });
        }
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "Classification rule added with {} scope ({} rule(s) now).",
                scope,
                ws_config.borrow().workspace.rules.len()
            ),
            "info",
        );
        save_config(&ws_config, "classification rule added");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Scope selected for the next rule. Choosing "local" without a working
    // directory is refused with the same notice the other directory actions use.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    let ws_island = island.clone();
    app.global::<WorkspaceBridge>()
        .on_rule_scope_chosen(move |scope| {
            let Some(app) = ws_handle.upgrade() else {
                return;
            };
            let scope = scope.to_string();
            let local = scope == config::RULE_SCOPE_LOCAL;
            if local
                && ws_config
                    .borrow()
                    .workspace
                    .directory
                    .trim()
                    .is_empty()
            {
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    "A local scope needs a working directory first.",
                    "warn",
                );
                dynamic_island::notify(
                    &ws_island,
                    ws_locale.borrow().t("workspace-need-directory").to_string(),
                    true,
                );
                return;
            }
            ws_state.borrow_mut().scope = scope.clone();
            app.set_workspace_rule_scope(scope.clone().into());
            logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("Scope of the next rule set to \"{}\".", scope),
                "info",
            );
        });

    // Per-row chip: moves an existing rule between global and local scope.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    let ws_island = island.clone();
    app.global::<WorkspaceBridge>()
        .on_rule_scope_toggled(move |index| {
            let Some(app) = ws_handle.upgrade() else {
                return;
            };
            let index = index.max(0) as usize;
            let directory = ws_config.borrow().workspace.directory.clone();
            {
                let mut guard = ws_config.borrow_mut();
                let Some(config_index) = ui_rule_index(&guard, index) else {
                    return;
                };
                let local = guard.workspace.rules[config_index].scope
                    == config::RULE_SCOPE_LOCAL;
                if !local && directory.trim().is_empty() {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        "A local rule needs a working directory first.",
                        "warn",
                    );
                    dynamic_island::notify(
                        &ws_island,
                        ws_locale.borrow().t("workspace-need-directory").to_string(),
                        true,
                    );
                    return;
                }
                let changed_to = if local {
                    config::RULE_SCOPE_GLOBAL
                } else {
                    config::RULE_SCOPE_LOCAL
                };
                let rule = &mut guard.workspace.rules[config_index];
                rule.scope = changed_to.to_string();
                if local {
                    rule.directory.clear();
                } else {
                    rule.directory = directory;
                }
                logs::logs_output::logs_output(
                    LOG_LOCATION,
                    &format!("Rule {} scope changed to \"{}\".", index + 1, changed_to),
                    "info",
                );
            }
            save_config(&ws_config, "classification rule scope changed");
            refresh_workspace(
                &app,
                &ws_config.borrow(),
                &ws_locale.borrow(),
                &ws_state.borrow(),
            );
        });

    let ws_config = config.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>()
        .on_rule_kind_chosen(move |index, kind| {
            let index = index.max(0) as usize;
            let kind = kind.to_string();
            let mut config = ws_config.borrow_mut();
            let Some(config_index) = ui_rule_index(&config, index) else {
                return;
            };
            let Some(rule) = config.workspace.rules.get_mut(config_index) else {
                return;
            };
            rule.kind = kind.clone();
            logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!(
                    "Rule {} now matches by {}.",
                    index + 1,
                    if kind == "ext" { "extension" } else { "keyword" }
                ),
                "info",
            );
            drop(config);
            save_config(&ws_config, "classification rule kind changed");
            // Only this row changes, so the model row is patched in place: a
            // full rebuild would recreate the row and the sliding highlight
            // would jump instead of animating to the other segment.
            let model = ws_state.borrow().kinds.borrow().clone();
            if let Some(model) = model {
                if (index as i32) < model.row_count() as i32 {
                    model.set_row_data(index, SharedString::from(kind));
                }
            }
        });

    // Typing into a rule value is a transient edit: it is kept in memory and only
    // written to disk together with the next real change, so a long keyword does
    // not cause one file write per keystroke.
    let ws_config = config.clone();
    app.global::<WorkspaceBridge>()
        .on_rule_value_edited(move |index, value| {
            let index = index.max(0) as usize;
            let mut config = ws_config.borrow_mut();
            let Some(config_index) = ui_rule_index(&config, index) else {
                return;
            };
            if let Some(rule) = config.workspace.rules.get_mut(config_index) {
                rule.value = value.to_string();
            }
        });

    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_rule_target(move |index| {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let index = index.max(0) as usize;
        let title = ws_locale.borrow().t("workspace-rule-target").to_string();
        let owner = crate::windows::window_center::main_window_handle();
        let Some(folder) = launchpad::dialog::pick_folder(owner, &title) else {
            logs::logs_output::logs_output(LOG_LOCATION, "Choosing a rule target was cancelled.", "info");
            return;
        };
        {
            let mut config = ws_config.borrow_mut();
            let Some(config_index) = ui_rule_index(&config, index) else {
                return;
            };
            let Some(rule) = config.workspace.rules.get_mut(config_index) else {
                return;
            };
            rule.target = folder.clone();
        }
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Rule {} now copies to \"{}\".", index + 1, folder),
            "info",
        );
        save_config(&ws_config, "classification rule target changed");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_rule_removed(move |index| {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let index = index.max(0) as usize;
        let removed = {
            let mut config = ws_config.borrow_mut();
            match ui_rule_index(&config, index) {
                Some(config_index) => Some(config.workspace.rules.remove(config_index)),
                None => None,
            }
        };
        let Some(removed) = removed else {
            return;
        };
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "Classification rule {} removed (kind {}, value \"{}\", target \"{}\").",
                index + 1,
                removed.kind,
                removed.value,
                removed.target
            ),
            "info",
        );
        save_config(&ws_config, "classification rule removed");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // The fallback rule: every file no ordinary rule matched is copied into one
    // folder, so nothing that was added can silently stay behind.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_other_choose(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let title = ws_locale.borrow().t("workspace-other-label").to_string();
        let owner = crate::windows::window_center::main_window_handle();
        let Some(folder) = launchpad::dialog::pick_folder(owner, &title) else {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Choosing the fallback folder was cancelled.",
                "info",
            );
            return;
        };
        {
            let mut config = ws_config.borrow_mut();
            // Only the fallback rule of the current directory is changed; the
            // fallback of another directory must stay untouched.
            let directory = config.workspace.directory.clone();
            let existing = visible_rule_indices(&config)
                .into_iter()
                .find(|index| config.workspace.rules[*index].kind == "other");
            match existing {
                Some(index) => config.workspace.rules[index].target = folder.clone(),
                None => config.workspace.rules.push(config::ClassifyRule {
                    kind: "other".to_string(),
                    value: String::new(),
                    target: folder.clone(),
                    scope: config::RULE_SCOPE_GLOBAL.to_string(),
                    directory,
                }),
            }
        }
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Fallback rule set to \"{}\".", folder),
            "info",
        );
        save_config(&ws_config, "fallback rule changed");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    app.global::<WorkspaceBridge>().on_other_clear(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        {
            let mut config = ws_config.borrow_mut();
            let directory = config.workspace.directory.clone();
            config
                .workspace
                .rules
                .retain(|rule| !(rule.kind == "other" && rule_visible(rule, &directory)));
        }
        logs::logs_output::logs_output(LOG_LOCATION, "Fallback rule removed.", "info");
        save_config(&ws_config, "fallback rule removed");
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // Run the classifier over the staged paths.
    let ws_handle = app.as_weak();
    let ws_config = config.clone();
    let ws_locale = locale.clone();
    let ws_state = workspace_ui.clone();
    let ws_island = island.clone();
    app.global::<WorkspaceBridge>().on_process(move || {
        let Some(app) = ws_handle.upgrade() else {
            return;
        };
        let (directory, rules) = {
            let config = ws_config.borrow();
            (
                config.workspace.directory.clone(),
                rules_for_directory(&config),
            )
        };
        let Some(workdir) = workspace::usable_directory(&directory) else {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Processing needs a working directory first.",
                "warn",
            );
            let message = ws_locale.borrow().t("workspace-need-directory").to_string();
            app.set_workspace_result(message.clone().into());
            app.set_workspace_result_error(true);
            dynamic_island::notify(&ws_island, message, true);
            return;
        };
        let sources = ws_state.borrow().added.clone();
        if sources.is_empty() {
            logs::logs_output::logs_output(
                LOG_LOCATION,
                "Processing was requested with nothing staged.",
                "warn",
            );
            let message = ws_locale.borrow().t("workspace-nothing-staged").to_string();
            app.set_workspace_result(message.clone().into());
            app.set_workspace_result_error(true);
            dynamic_island::notify(&ws_island, message, true);
            return;
        }
        let outcome = workspace::classify::process(&sources, &rules, &workdir);
        // `str::replace` rewrites every placeholder at once, so the three counts
        // have to be filled one occurrence at a time.
        let summary = ws_locale
            .borrow()
            .t("workspace-process-done")
            .replacen("{}", &outcome.copied.to_string(), 1)
            .replacen("{}", &outcome.skipped.to_string(), 1)
            .replacen("{}", &outcome.failed.to_string(), 1);
        app.set_workspace_result(summary.clone().into());
        app.set_workspace_result_error(outcome.failed > 0);
        let failed = outcome.failed > 0;
        dynamic_island::notify(&ws_island, summary, failed);
        refresh_workspace(
            &app,
            &ws_config.borrow(),
            &ws_locale.borrow(),
            &ws_state.borrow(),
        );
    });

    // --- Navigation: remember the visible page ----------------------------
    let page_config = config.clone();
    let page_app = app.as_weak();
    app.global::<NavigationBridge>().on_page_changed(move |page| {
        let page = page.clamp(0, 2);
        {
            page_config.borrow_mut().page.main = page;
        }
        // The incoming page must exist *before* the strip starts moving,
        // otherwise the slide would reveal an empty cell.
        if let Some(app) = page_app.upgrade() {
            match page {
                0 => app.set_page_alive_workspace(true),
                1 => app.set_page_alive_launchpad(true),
                _ => app.set_page_alive_settings(true),
            }
            // The page that slides out is still alive right now; drop it once
            // the animation is over, so a switch holds two pages and the idle
            // window only one.
            let cleanup_app = app.as_weak();
            Timer::single_shot(
                Duration::from_millis(PAGE_SLIDE_MS + PAGE_DROP_MARGIN_MS),
                move || {
                    let Some(app) = cleanup_app.upgrade() else {
                        return;
                    };
                    // Read the page again: the user may have clicked a second
                    // tab while the first slide was still running.
                    let current = app.get_operation_area_page();
                    app.set_page_alive_workspace(current == 0);
                    app.set_page_alive_launchpad(current == 1);
                    app.set_page_alive_settings(current == 2);
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Page {} finished sliding in; the other pages are released.", current),
                        "info",
                    );
                },
            );
        }
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!("Page {} is now the visible page.", page),
            "info",
        );
        save_config(&page_config, "page changed");
    });

    // --- Navigation: remember the selected settings row -------------------
    let settings_config = config.clone();
    app.global::<NavigationBridge>()
        .on_settings_page_changed(move |page| {
            let page = page.clamp(0, 1);
            {
                settings_config.borrow_mut().page.settings = page;
            }
            logs::logs_output::logs_output(
                LOG_LOCATION,
                &format!("Settings row {} is now selected.", page),
                "info",
            );
            save_config(&settings_config, "settings page changed");
        });

    logs::logs_output::logs_output(LOG_LOCATION, "Showing the main window.", "info");
    app.show().expect("Failed to show the main window");

    // The native window only gets its final size once the event loop runs and
    // the first layout happened, so the centering polls until the window can be
    // measured and then stops itself.
    let center_timer = std::rc::Rc::new(Timer::default());
    {
        let timer = center_timer.clone();
        let timer_in_callback = center_timer.clone();
        let center_handle = app.as_weak();
        let attempts = std::rc::Rc::new(std::cell::Cell::new(0u32));

        // Captured for the drop target: the window only gets a usable handle once
        // it is laid out, which is exactly when centering succeeds below.
        let drop_config = config.clone();
        let drop_locale = locale.clone();
        let drop_island = island.clone();
        let drop_workspace = workspace_ui.clone();
        let drop_installed = std::rc::Rc::new(std::cell::Cell::new(false));

        timer.start(
            slint::TimerMode::Repeated,
            std::time::Duration::from_millis(CENTER_POLL_INTERVAL),
            move || {
                attempts.set(attempts.get() + 1);

                let Some(app) = center_handle.upgrade() else {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        "Window vanished before it could be centered, giving up.",
                        "warn",
                    );
                    timer_in_callback.stop();
                    return;
                };

                if crate::windows::window_center::center_window(&app.window()) {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!("Window centered after {} poll(s).", attempts.get()),
                        "info",
                    );

                    // The window now has its final size and position, so this is
                    // the moment the frameless window can be given the rounded
                    // corners of a native one.
                    window_controls::apply_rounded_corners();

                    // Same moment, same reason: only now does the handle search
                    // find the real window, so the drop target is installed here.
                    if !drop_installed.replace(true) {
                        let handler_config = drop_config.clone();
                        let handler_locale = drop_locale.clone();
                        let handler_island = drop_island.clone();
                        let handler_workspace = drop_workspace.clone();
                        launchpad::drop_target::install(&app, move |app| {
                            let paths = launchpad::drop_target::take_pending();
                            if paths.is_empty() {
                                return;
                            }
                            // A drop means different things on different pages:
                            // on the workspace it stages the files for the
                            // classifier, on the launchpad it adds tiles. On the
                            // settings page there is nothing to add to, so the
                            // drop is ignored (and only logged) instead of
                            // silently landing in the launchpad.
                            let page = app.get_operation_area_page();
                            if page == 0 {
                                let added = {
                                    let mut state = handler_workspace.borrow_mut();
                                    let mut added = 0usize;
                                    for path in &paths {
                                        if !state
                                            .added
                                            .iter()
                                            .any(|existing| existing.eq_ignore_ascii_case(path))
                                        {
                                            state.added.push(path.clone());
                                            added += 1;
                                        }
                                    }
                                    added
                                };
                                logs::logs_output::logs_output(
                                    LOG_LOCATION,
                                    &format!(
                                        "Dropped {} path(s) on the workspace page, {} new to the list.",
                                        paths.len(),
                                        added
                                    ),
                                    "info",
                                );
                                refresh_workspace(
                                    &app,
                                    &handler_config.borrow(),
                                    &handler_locale.borrow(),
                                    &handler_workspace.borrow(),
                                );
                                if added > 0 {
                                    let text = notice_text(
                                        &handler_locale.borrow(),
                                        "workspace-notice-added",
                                        &added.to_string(),
                                    );
                                    dynamic_island::notify(&handler_island, text, false);
                                }
                            } else if page == 1 {
                                logs::logs_output::logs_output(
                                    LOG_LOCATION,
                                    &format!(
                                        "Dropped {} path(s) are added to the launchpad.",
                                        paths.len()
                                    ),
                                    "info",
                                );
                                let locale = handler_locale.borrow();
                                add_paths(app, &handler_config, &locale, &handler_island, &paths);
                            } else {
                                logs::logs_output::logs_output(
                                    LOG_LOCATION,
                                    &format!(
                                        "Dropped {} path(s) on the settings page are ignored; drop them on the workspace or the launchpad.",
                                        paths.len()
                                    ),
                                    "warn",
                                );
                            }
                        });
                    }


                    timer_in_callback.stop();
                } else if attempts.get() > CENTER_MAX_ATTEMPTS {
                    logs::logs_output::logs_output(
                        LOG_LOCATION,
                        &format!(
                            "No laid out window after {} polls ({}ms), centering skipped.",
                            attempts.get(),
                            u64::from(CENTER_MAX_ATTEMPTS) * CENTER_POLL_INTERVAL
                        ),
                        "err",
                    );
                    timer_in_callback.stop();
                }
            },
        );
    }

    logs::logs_output::logs_output(LOG_LOCATION, "Entering the event loop.", "info");
    let event_loop_result = slint::run_event_loop();
    logs::logs_output::logs_output(
        LOG_LOCATION,
        "Event loop finished, the main window was closed.",
        "info",
    );

    // Remember what the user was looking at when the window was closed. Every
    // switch already saved, this final write also covers a configuration that
    // was changed by hand while the application was running.
    {
        let guard = config.borrow();
        logs::logs_output::logs_output(
            LOG_LOCATION,
            &format!(
                "Session state: page {}/{}, {} launchpad tile(s).",
                guard.page.main,
                guard.page.settings,
                guard.launchpad.items.len()
            ),
            "info",
        );
    }
    save_config(&config, "shutdown");

    // Keep the timers alive until the loop returned; dropping them earlier would
    // silently stop the clock and the centering polls.
    drop(clock_timer);
    drop(center_timer);

    logs::logs_output::logs_output(LOG_LOCATION, "MainWindow stopping.", "info");
    event_loop_result?;
    Ok(())
}
