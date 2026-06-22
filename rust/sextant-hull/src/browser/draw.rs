//! Software renderer for the sextant-browser shell — every `draw_*` panel,
//! the top bar / controls / tab strip / AI rail / status bar, and the
//! low-level rasterization primitives (fill/stroke rect, clipped text, glyph
//! blitting). Free functions split out of browser.rs; `use super::*` supplies
//! BrowserApp (its methods are pub), the data types, and theme colors.
//! Re-exported from the crate root (`use draw::*`) so the run loops and other
//! modules call these unchanged.

use super::*;

pub fn draw(
    window: &Window,
    surface: &mut Surface<Arc<Window>, Arc<Window>>,
    surface_size: &mut PhysicalSize<u32>,
    app: &BrowserApp,
) -> Result<DrawStats, String> {
    let draw_started = Instant::now();
    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    if surface_size.width != width || surface_size.height != height {
        surface
            .resize(
                NonZeroU32::new(width).ok_or("invalid width")?,
                NonZeroU32::new(height).ok_or("invalid height")?,
            )
            .map_err(|e| e.to_string())?;
        *surface_size = PhysicalSize::new(width, height);
    }

    let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
    for pixel in buffer.iter_mut() {
        *pixel = BG;
    }

    draw_top_bar(&mut buffer, width, height, app);
    draw_controls(&mut buffer, width, height, app);
    draw_page_tab_strip(&mut buffer, width, height, app);
    draw_page_load_bar(&mut buffer, width, height, app);
    draw_metric_cards(&mut buffer, width, height, app);
    let frame_blit = match app.main_view {
        MainView::Browser => draw_page_panel(&mut buffer, width, height, app),
        MainView::Wake => {
            draw_wake_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Log => {
            draw_log_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Guard => {
            draw_guard_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Perception => {
            draw_perception_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Perf => {
            draw_perf_panel(&mut buffer, width, height, app);
            None
        }
        MainView::Validation => {
            draw_validation_panel(&mut buffer, width, height, app);
            None
        }
        #[cfg(feature = "xilem-shell")]
        MainView::Settings => {
            draw_settings_panel(&mut buffer, width, height, app);
            None
        }
    };
    draw_ai_rail(&mut buffer, width, height, app);
    draw_status_bar(&mut buffer, width, height, app);

    let present_started = Instant::now();
    buffer.present().map_err(|e| e.to_string())?;
    Ok(DrawStats {
        total: draw_started.elapsed(),
        frame_blit,
        present: present_started.elapsed(),
    })
}

pub fn draw_top_bar(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: 0,
            w: width,
            h: CHROME_H,
        },
        PANEL_DARK,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: CHROME_H.saturating_sub(1),
            w: width,
            h: 1,
        },
        BORDER,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: 0,
            w: rail_x,
            h: 2,
        },
        BUTTON_BRIGHT,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 0,
            w: RAIL_WIDTH,
            h: 2,
        },
        STATUS_OK,
    );
    draw_text(buffer, width, height, 24, 12, "SEXTANT", TEXT, 1);
    draw_text(buffer, width, height, 24, 32, "AI BROWSER", TEXT_DIM, 1);
    for region in &app.mode_rects {
        draw_pill(
            buffer,
            width,
            height,
            region.rect,
            region.mode.label(),
            region.mode == app.browser_mode,
        );
    }
    let tabs = app.engine.get_tabs();
    let tab_label = format!("TABS {}", tabs.len());
    draw_text(
        buffer,
        width,
        height,
        rail_x + 198,
        24,
        &tab_label,
        TEXT_DIM,
        1,
    );
    draw_status_dot(buffer, width, height, rail_x + 18, 23, STATUS_OK);
    draw_text(buffer, width, height, rail_x + 34, 15, "MAYA", TEXT, 1);
    draw_text(
        buffer,
        width,
        height,
        rail_x + 34,
        33,
        "LOCAL ONLINE",
        STATUS_OK,
        1,
    );
}

pub fn draw_controls(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: CHROME_H,
            w: rail_x,
            h: STRIP_H,
        },
        PANEL_SOFT,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: CHROME_H + STRIP_H - 1,
            w: rail_x,
            h: 1,
        },
        BORDER,
    );
    draw_field_with_placeholder(
        buffer,
        width,
        height,
        app.address_rect,
        &app.address_input,
        "ENTER URL OR ASK SEXTANT",
        app.focus == FocusTarget::Address,
    );

    for button in &app.buttons {
        let hovered = app
            .cursor
            .map(|(x, y)| button.rect.contains(x, y))
            .unwrap_or(false);
        draw_button(
            buffer,
            width,
            height,
            button.rect,
            button.label,
            hovered,
            app.action_enabled(button.action),
        );
    }

    draw_pill(
        buffer,
        width,
        height,
        app.browser_tab_rect,
        "BROWSER",
        app.main_view == MainView::Browser,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.wake_tab_rect,
        "WAKE",
        app.main_view == MainView::Wake,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.log_tab_rect,
        "LOG",
        app.main_view == MainView::Log,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.guard_tab_rect,
        "GUARD",
        app.main_view == MainView::Guard,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.perception_tab_rect,
        "SENSE",
        app.main_view == MainView::Perception,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.perf_tab_rect,
        "PERF",
        app.main_view == MainView::Perf,
    );
    draw_pill(
        buffer,
        width,
        height,
        app.validation_tab_rect,
        "VALIDATION",
        app.main_view == MainView::Validation,
    );
    #[cfg(feature = "xilem-shell")]
    draw_pill(
        buffer,
        width,
        height,
        app.settings_tab_rect,
        "SETTINGS",
        app.main_view == MainView::Settings,
    );
    let ai_ready = app.capabilities().ai_observe_dom;
    draw_status_chip(
        buffer,
        width,
        height,
        Rect {
            x: rail_x.saturating_sub(102),
            y: CHROME_H + STRIP_H + 8,
            w: 84,
            h: 24,
        },
        if ai_ready { "AI READY" } else { "AI OFF" },
        if ai_ready { STATUS_OK } else { TEXT_DIM },
        ai_ready,
    );
}

pub fn draw_page_tab_strip(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let strip = Rect {
        x: 0,
        y: CHROME_H + STRIP_H + TAB_H,
        w: rail_x,
        h: PAGE_TAB_H,
    };
    fill_rect(buffer, width, height, strip, PANEL_DARK);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y: strip.y + strip.h.saturating_sub(1),
            w: rail_x,
            h: 1,
        },
        BORDER,
    );

    let tabs = app.engine.get_tabs();
    let active_id = app.active_tab().map(|tab| tab.id);
    if tabs.len() > app.page_tab_rects.len() {
        draw_page_tab_pager(
            buffer,
            width,
            height,
            app.page_tab_prev_rect,
            "<",
            app.can_page_tabs_previous(),
            app.cursor
                .map(|(x, y)| app.page_tab_prev_rect.contains(x, y))
                .unwrap_or(false),
        );
        draw_page_tab_pager(
            buffer,
            width,
            height,
            app.page_tab_next_rect,
            ">",
            app.can_page_tabs_next(),
            app.cursor
                .map(|(x, y)| app.page_tab_next_rect.contains(x, y))
                .unwrap_or(false),
        );
    }
    for region in &app.page_tab_rects {
        let Some(tab) = tabs.iter().find(|tab| tab.id == region.tab_id) else {
            continue;
        };
        let active = Some(tab.id) == active_id;
        let hovered = app
            .cursor
            .map(|(x, y)| region.rect.contains(x, y))
            .unwrap_or(false);
        draw_page_tab(buffer, width, height, region.rect, tab, active, hovered);
    }

    if tabs.len() > app.page_tab_rects.len() {
        if let Some((rect, label)) = page_tab_range_label_rect(app) {
            draw_text(buffer, width, height, rect.x, rect.y, &label, TEXT_DIM, 1);
        }
    }
}

pub fn draw_page_load_bar(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let track = page_load_bar_rect(width);
    if track.w == 0 || track.h == 0 {
        return;
    }

    fill_rect(buffer, width, height, track, FIELD);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: track.x,
            y: track.y,
            w: track.w,
            h: 1,
        },
        BORDER,
    );

    if let Some(active) = page_load_bar_active_rect(app, track) {
        fill_rect(buffer, width, height, active, BUTTON_BRIGHT);
        let pulse = Rect {
            x: active.x,
            y: active.y,
            w: active.w,
            h: 1,
        };
        fill_rect(buffer, width, height, pulse, STATUS_OK);
    }
}

pub fn page_load_bar_rect(width: u32) -> Rect {
    let rail_x = right_rail_x(width);
    Rect {
        x: 24,
        y: CHROME_H + STRIP_H + TAB_H + PAGE_TAB_H + 4,
        w: rail_x.saturating_sub(48),
        h: LOAD_BAR_H,
    }
}

pub fn page_load_bar_active_rect(app: &BrowserApp, track: Rect) -> Option<Rect> {
    let pending = app.pending_navigation.as_ref()?;
    if track.w == 0 {
        return None;
    }

    let segment_w = (track.w / 3).max(64).min(track.w);
    let travel = track.w.saturating_sub(segment_w);
    let elapsed_ms = pending.started.elapsed().as_millis() as u32;
    let offset = if travel == 0 {
        0
    } else {
        let cycle = travel.saturating_mul(2).max(1);
        let phase = (elapsed_ms / 8) % cycle;
        if phase <= travel {
            phase
        } else {
            cycle.saturating_sub(phase)
        }
    };

    Some(Rect {
        x: track.x.saturating_add(offset),
        y: track.y,
        w: segment_w,
        h: track.h,
    })
}

pub fn page_tab_range_label_rect(app: &BrowserApp) -> Option<(Rect, String)> {
    let tabs_len = app.engine.get_tabs().len();
    if tabs_len <= app.page_tab_rects.len() || app.page_tab_rects.is_empty() {
        return None;
    }
    let first = app.page_tab_window_start + 1;
    let last = (app.page_tab_window_start + app.page_tab_rects.len()).min(tabs_len);
    let label = format!("{first}-{last}/{tabs_len}");
    let w = text_width(&label, 1);
    let h = char_advance(1);
    let x = app.page_tab_next_rect.x.saturating_sub(w + 8);
    let y = app.page_tab_next_rect.y + 9;
    let rect = Rect { x, y, w, h };
    let minimum_x = app
        .page_tab_rects
        .last()
        .map(|region| {
            region
                .rect
                .x
                .saturating_add(region.rect.w)
                .saturating_add(8)
        })
        .unwrap_or(
            app.page_tab_prev_rect
                .x
                .saturating_add(app.page_tab_prev_rect.w),
        );
    if rect.x < minimum_x || rect.intersects(app.page_tab_next_rect) {
        return None;
    }
    Some((rect, label))
}

pub fn draw_page_tab_pager(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    enabled: bool,
    hovered: bool,
) {
    let fill = if enabled && hovered {
        FIELD_FOCUS
    } else {
        FIELD
    };
    fill_rect(buffer, width, height, rect, fill);
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if enabled { BORDER } else { BUTTON_DISABLED },
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 9,
        rect.y + 9,
        label,
        if enabled { TEXT } else { TEXT_DIM },
        1,
    );
}

pub fn draw_page_tab(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    tab: &Tab,
    active: bool,
    hovered: bool,
) {
    let fill = if active {
        PANEL_ALT
    } else if hovered {
        FIELD_FOCUS
    } else {
        FIELD
    };
    fill_rect(buffer, width, height, rect, fill);
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { BUTTON_ACTIVE } else { BORDER },
    );
    let marker = if active { STATUS_OK } else { TEXT_DIM };
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x + 8,
            y: rect.y + 8,
            w: 6,
            h: 6,
        },
        marker,
    );
    let label = page_tab_label(tab);
    let max_chars = ((rect.w.saturating_sub(28)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        rect.x + 20,
        rect.y + 9,
        &truncate(&label, max_chars),
        if active { TEXT } else { TEXT_DIM },
        1,
    );
}

pub fn draw_metric_cards(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let left = 24;
    let gap = 12;
    let card_w = ((rail_x.saturating_sub(left * 2 + gap * 3)) / 4).max(120);
    let tabs = app.engine.get_tabs();
    let wake_count = app.wake_results.len();
    let backend_label = app
        .active_tab()
        .map(|tab| display_backend(app, tab).to_string())
        .unwrap_or_else(|| "NONE".to_string());
    let page_label = app
        .active_tab()
        .and_then(|tab| tab.distilled_page.as_ref())
        .map(|_| "READY".to_string())
        .unwrap_or_else(|| "PENDING".to_string());

    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left,
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "TABS",
        &tabs.len().to_string(),
        "ACTIVE SESSION",
        STATUS_OK,
    );
    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left + (card_w + gap),
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "ENGINE",
        &backend_label,
        "LOCAL BUILD",
        BUTTON_BRIGHT,
    );
    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left + (card_w + gap) * 2,
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "WAKE",
        &wake_count.to_string(),
        "SEARCH HITS",
        if wake_count > 0 { STATUS_OK } else { TEXT_DIM },
    );
    draw_metric_card(
        buffer,
        width,
        height,
        Rect {
            x: left + (card_w + gap) * 3,
            y: METRIC_Y,
            w: card_w,
            h: METRIC_H,
        },
        "PAGE",
        &page_label,
        "DISTILL STATUS",
        if page_label == "READY" {
            STATUS_OK
        } else {
            STATUS_WARN
        },
    );
}

pub fn draw_page_panel(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    app: &BrowserApp,
) -> Option<Duration> {
    let rail_x = right_rail_x(width);
    let page = main_panel_rect(rail_x, height);
    let panel = Rect {
        x: 24,
        y: page.y,
        w: rail_x.saturating_sub(48),
        h: page.h,
    };
    draw_panel_surface(buffer, width, height, panel, BUTTON_BRIGHT);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 20,
        "ACTIVE PAGE",
        TEXT,
        1,
    );

    let status_color = if app.last_ok { STATUS_OK } else { STATUS_WARN };
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 44,
            y: panel.y + 46,
            w: 10,
            h: 10,
        },
        status_color,
    );
    draw_text(
        buffer,
        width,
        height,
        62,
        panel.y + 44,
        &truncate(
            &app.last_status,
            ((panel.w.saturating_sub(62)) / 8) as usize,
        ),
        TEXT,
        1,
    );

    if let Some(tab) = app.active_tab() {
        let url = tab
            .url
            .as_ref()
            .map(short_url)
            .unwrap_or_else(|| "about:blank".to_string());
        let status = format!(
            "TAB {} | BACK {} | FORWARD {} | ENGINE {} | RENDER {}",
            short_id(tab.id),
            tab.can_go_back,
            tab.can_go_forward,
            display_backend(app, tab),
            app.render_path_label()
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 76,
            &truncate(&url, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 100,
            &truncate(&status, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
        if let Some(frame) = app.latest_frame.as_ref() {
            return Some(draw_rendered_frame(buffer, width, height, panel, frame));
        } else if let Some(page) = tab.distilled_page.as_ref() {
            draw_reader_page(buffer, width, height, panel, page, app.page_scroll);
        } else {
            draw_text(
                buffer,
                width,
                height,
                44,
                panel.y + 134,
                if cfg!(feature = "servo-backend") {
                    "NO SERVO FRAME YET"
                } else {
                    "NO DISTILLED PAGE YET"
                },
                TEXT,
                1,
            );
            draw_text(
                buffer,
                width,
                height,
                44,
                panel.y + 160,
                if cfg!(feature = "servo-backend") {
                    "PRESS GO OR DISTILL TO CAPTURE THE LIVE VIEWPORT."
                } else {
                    "PRESS DISTILL TO FETCH AND PARSE THE CURRENT URL."
                },
                TEXT_DIM,
                1,
            );
        }
    } else {
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: 32,
                y: panel.y + 70,
                w: panel.w.saturating_sub(64),
                h: 106,
            },
            PANEL_DARK,
        );
        stroke_rect(
            buffer,
            width,
            height,
            Rect {
                x: 32,
                y: panel.y + 70,
                w: panel.w.saturating_sub(64),
                h: 106,
            },
            BORDER_SOFT,
        );
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: 32,
                y: panel.y + 70,
                w: 2,
                h: 106,
            },
            BUTTON_BRIGHT,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 86,
            "READY FOR A PAGE",
            TEXT,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 112,
            "ENTER A URL OR ASK SEXTANT FROM THE ADDRESS BAR.",
            TEXT_SOFT,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 138,
            "THE AI SIDECAR WILL TRACK CONTEXT WITHOUT TAKING CONTROL.",
            TEXT_DIM,
            1,
        );
    }
    None
}

pub fn draw_wake_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_OK);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "DIGITAL WAKE",
        TEXT,
        1,
    );

    if app.wake_results.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 54,
            "NO WAKE RESULTS YET. DISTILL A PAGE OR ENTER A QUERY IN THE AI RAIL.",
            TEXT_DIM,
            1,
        );
        return;
    }

    let max_rows = panel.h.saturating_sub(56) / 30;
    for (index, entry) in app.wake_results.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 52 + index as u32 * 30;
        let line = format!("{} | {}", entry.title, short_url(&entry.url));
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            &truncate(&line, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
        let detail = format!(
            "IMPORTANCE {:.2} | USED {}",
            entry.importance, entry.usage_count
        );
        draw_text(
            buffer,
            width,
            height,
            44,
            y + 14,
            &truncate(&detail, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
    }
}

pub fn draw_reader_page(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    panel: Rect,
    page: &sextant_engine::DistilledPage,
    scroll: i32,
) {
    let clip = Rect {
        x: panel.x + 18,
        y: panel.y + 126,
        w: panel.w.saturating_sub(36),
        h: panel.h.saturating_sub(142),
    };
    fill_rect(buffer, width, height, clip, PANEL_ALT);
    let max_chars = ((clip.w.saturating_sub(8)) / char_advance(1)) as usize;
    let max_title_chars = ((clip.w.saturating_sub(8)) / char_advance(2)) as usize;
    let mut y = clip.y as i32 - scroll;

    for line in wrap_text(&page.title, max_title_chars).into_iter().take(3) {
        draw_text_clipped(buffer, width, height, clip, clip.x, y, &line, TEXT, 2);
        y += 20;
    }
    y += 8;
    draw_text_clipped(
        buffer,
        width,
        height,
        clip,
        clip.x,
        y,
        &truncate(page.url.as_str(), max_chars),
        TEXT_DIM,
        1,
    );
    y += 28;

    for line in wrap_text(&page.content, max_chars).into_iter().take(24) {
        draw_text_clipped(buffer, width, height, clip, clip.x, y, &line, TEXT_DIM, 1);
        y += 18;
    }

    y += 14;
    draw_text_clipped(
        buffer,
        width,
        height,
        clip,
        clip.x,
        y,
        "SEMANTIC MAP",
        TEXT,
        1,
    );
    y += 24;
    let counts = semantic_counts(page);
    let semantic_line = format!(
        "HEADINGS {} | LINKS {} | INPUTS {} | IMAGES {} | TEXT NODES {}",
        counts.headings, counts.links, counts.inputs, counts.images, counts.text
    );
    draw_text_clipped(
        buffer,
        width,
        height,
        clip,
        clip.x,
        y,
        &truncate(&semantic_line, max_chars),
        TEXT_DIM,
        1,
    );
    y += 24;

    let key_nodes = page
        .semantic_map
        .iter()
        .filter(|node| matches!(node.node_type, NodeType::Heading | NodeType::Link))
        .take(2)
        .map(|node| node.text.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    if !key_nodes.is_empty() {
        draw_text_clipped(
            buffer,
            width,
            height,
            clip,
            clip.x,
            y,
            &truncate(&key_nodes, max_chars),
            TEXT_DIM,
            1,
        );
    }

    if scroll > 0 {
        draw_text(
            buffer,
            width,
            height,
            panel.x + panel.w - 108,
            panel.y + 20,
            "SCROLLED",
            TEXT_DIM,
            1,
        );
    } else {
        draw_text(
            buffer,
            width,
            height,
            panel.x + panel.w - 112,
            panel.y + 20,
            "WHEEL TO READ",
            TEXT_DIM,
            1,
        );
    }
}

pub fn draw_rendered_frame(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    panel: Rect,
    frame: &RenderedFrame,
) -> Duration {
    let started = Instant::now();
    let viewport = browser_viewport_rect(panel);
    fill_rect(buffer, width, height, viewport, FIELD);
    if frame.width == 0 || frame.height == 0 || frame.pixels.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            viewport.x + 12,
            viewport.y + 12,
            "SERVO FRAME IS EMPTY",
            STATUS_WARN,
            1,
        );
        return started.elapsed();
    }
    let expected_pixels = frame.width as usize * frame.height as usize;
    if frame.pixels.len() < expected_pixels {
        draw_text(
            buffer,
            width,
            height,
            viewport.x + 12,
            viewport.y + 12,
            "SERVO FRAME IS TRUNCATED",
            STATUS_WARN,
            1,
        );
        return started.elapsed();
    }

    let scale_x = viewport.w as f32 / frame.width as f32;
    let scale_y = viewport.h as f32 / frame.height as f32;
    let scale = scale_x.min(scale_y).max(0.01);
    let draw_w = (frame.width as f32 * scale).max(1.0) as u32;
    let draw_h = (frame.height as f32 * scale).max(1.0) as u32;
    let offset_x = viewport.x + viewport.w.saturating_sub(draw_w) / 2;
    let offset_y = viewport.y + viewport.h.saturating_sub(draw_h) / 2;

    let copy_w = draw_w.min(viewport.w).min(width.saturating_sub(offset_x));
    let copy_h = draw_h.min(viewport.h).min(height.saturating_sub(offset_y));
    if copy_w > 0 && copy_h > 0 {
        let x_step = ((frame.width as u64) << 32) / draw_w.max(1) as u64;
        let y_step = ((frame.height as u64) << 32) / draw_h.max(1) as u64;
        for dy in 0..copy_h {
            let src_y = (((dy as u64 * y_step) >> 32) as u32).min(frame.height - 1);
            let src_row_start = src_y as usize * frame.width as usize;
            let dest_y = offset_y + dy;
            let dest_start = dest_y as usize * width as usize + offset_x as usize;
            let dest_end = dest_start + copy_w as usize;
            if let Some(dest_row) = buffer.get_mut(dest_start..dest_end) {
                for dx in 0..copy_w {
                    let src_x = (((dx as u64 * x_step) >> 32) as u32).min(frame.width - 1);
                    dest_row[dx as usize] = frame.pixels[src_row_start + src_x as usize];
                }
            }
        }
    }

    stroke_rect(buffer, width, height, viewport, BUTTON_ACTIVE);
    let label = format!("SERVO FRAME {}x{}", frame.width, frame.height);
    draw_text(
        buffer,
        width,
        height,
        panel.x + panel.w.saturating_sub(174),
        panel.y + 20,
        &label,
        TEXT_DIM,
        1,
    );
    started.elapsed()
}

pub fn draw_log_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, BUTTON_BRIGHT);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "CAPTAIN'S LOG",
        TEXT,
        1,
    );

    if app.recent_logs.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 54,
            "NO LOG ENTRIES YET",
            TEXT_DIM,
            1,
        );
        return;
    }

    let max_rows = panel.h.saturating_sub(50) / 22;
    for (index, entry) in app.recent_logs.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 52 + index as u32 * 22;
        let line = format!("{} | {}", status_label(&entry.status), entry.intent);
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            &truncate(&line, ((panel.w.saturating_sub(40)) / 8) as usize),
            TEXT_DIM,
            1,
        );
    }
}

pub fn draw_guard_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_WARN);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "LOCAL GUARD",
        TEXT,
        1,
    );

    let active_url = app
        .active_tab()
        .and_then(|tab| tab.url.as_ref())
        .or_else(|| {
            app.active_tab()
                .and_then(|tab| tab.distilled_page.as_ref())
                .map(|page| &page.url)
        });
    let active_page = app.active_tab().and_then(|tab| tab.distilled_page.as_ref());
    let lines = guard_report_lines(app, active_url, active_page);
    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    let cert_section_y = panel.y + GUARD_CERT_SECTION_Y;
    let max_rows = cert_section_y.saturating_sub(panel.y + 58) / 26;
    for (index, line) in lines.iter().take(max_rows as usize).enumerate() {
        let y = panel.y + 54 + index as u32 * 26;
        let color = if line.contains("BLOCK") || line.contains("HARDENED") {
            STATUS_WARN
        } else if line.contains("ALLOW") || line.contains("ONLINE") {
            STATUS_OK
        } else {
            TEXT_DIM
        };
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            &truncate(line, max_chars),
            color,
            1,
        );
    }
    draw_appliance_cert_settings(buffer, width, height, panel, app);
}

#[cfg(feature = "xilem-shell")]
pub fn settings_backend_button_rects(panel: Rect) -> Vec<(&'static str, Rect)> {
    let button_w = 150;
    let gap = 14;
    let y = panel.y + 78;
    AiLocalConfig::BACKENDS
        .iter()
        .enumerate()
        .map(|(index, backend)| {
            let x = panel.x + 24 + index as u32 * (button_w + gap);
            (
                *backend,
                Rect {
                    x,
                    y,
                    w: button_w,
                    h: 34,
                },
            )
        })
        .collect()
}

#[cfg(feature = "xilem-shell")]
pub fn draw_settings_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_OK);
    draw_text(
        buffer,
        width,
        height,
        panel.x + 20,
        panel.y + 18,
        "LOCAL AI MODEL",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        panel.y + 52,
        "BACKEND",
        TEXT_DIM,
        1,
    );

    let active = app.ai_config.backend.as_str();
    for (backend, rect) in settings_backend_button_rects(panel) {
        // Highlight the active backend by drawing it in the "hovered" style.
        let selected = backend == active;
        draw_button(
            buffer,
            width,
            height,
            rect,
            AiLocalConfig::backend_label(backend),
            selected,
            true,
        );
    }

    let info_y = panel.y + 138;
    let max_chars = (panel.w.saturating_sub(48) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y,
        &truncate(&format!("ENDPOINT  {}", app.ai_config.endpoint), max_chars),
        TEXT_SOFT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 26,
        &truncate(&format!("MODEL     {}", app.ai_config.model), max_chars),
        TEXT_SOFT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 52,
        &truncate(
            &format!(
                "ACTIVE    {} brain @ {}",
                AiLocalConfig::backend_label(active),
                app.ai_config.endpoint
            ),
            max_chars,
        ),
        STATUS_OK,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 92,
        "Click a backend to select it and rebuild the agent brain.",
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        panel.x + 24,
        info_y + 114,
        &truncate(
            "Endpoint/model use that backend's default; edit <profile>/ai-provider.json or set SEXTANT_LOCAL_ENDPOINT / SEXTANT_LOCAL_MODEL to customize.",
            max_chars,
        ),
        TEXT_DIM,
        1,
    );
}

pub fn guard_appliance_refresh_rect(panel: Rect) -> Rect {
    Rect {
        x: panel.x + panel.w.saturating_sub(226),
        y: panel.y + GUARD_CERT_SECTION_Y,
        w: 96,
        h: 28,
    }
}

pub fn guard_appliance_forget_rect(panel: Rect) -> Rect {
    Rect {
        x: panel.x + panel.w.saturating_sub(118),
        y: panel.y + GUARD_CERT_SECTION_Y,
        w: 92,
        h: 28,
    }
}

#[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
pub fn guard_appliance_row_rects(panel: Rect, count: usize) -> Vec<(usize, Rect)> {
    let row_start = panel.y + GUARD_CERT_SECTION_Y + 76;
    let max_rows = panel
        .y
        .saturating_add(panel.h)
        .saturating_sub(row_start + 8)
        / GUARD_CERT_ROW_H;
    (0..count.min(max_rows as usize))
        .map(|index| {
            (
                index,
                Rect {
                    x: panel.x + 20,
                    y: row_start + index as u32 * GUARD_CERT_ROW_H,
                    w: panel.w.saturating_sub(40),
                    h: GUARD_CERT_ROW_H.saturating_sub(8),
                },
            )
        })
        .collect()
}

pub fn draw_appliance_cert_settings(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    panel: Rect,
    app: &BrowserApp,
) {
    let section_y = panel.y + GUARD_CERT_SECTION_Y;
    let left = panel.x + 20;
    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        left,
        section_y,
        "LOCAL APPLIANCE CERTIFICATES",
        TEXT,
        1,
    );
    let refresh_rect = guard_appliance_refresh_rect(panel);
    let forget_rect = guard_appliance_forget_rect(panel);
    let refresh_hovered = app
        .cursor
        .map(|(x, y)| refresh_rect.contains(x, y))
        .unwrap_or(false);
    let forget_hovered = app
        .cursor
        .map(|(x, y)| forget_rect.contains(x, y))
        .unwrap_or(false);
    draw_button(
        buffer,
        width,
        height,
        refresh_rect,
        "REFRESH",
        refresh_hovered,
        true,
    );

    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    let can_forget = app
        .selected_appliance_cert
        .is_some_and(|index| index < app.appliance_cert_entries.len());
    #[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
    let can_forget = false;
    draw_button(
        buffer,
        width,
        height,
        forget_rect,
        "FORGET",
        forget_hovered,
        can_forget,
    );

    draw_text(
        buffer,
        width,
        height,
        left,
        section_y + 34,
        &truncate(&app.appliance_cert_status, max_chars),
        TEXT_DIM,
        1,
    );

    #[cfg(any(feature = "xilem-shell", feature = "servo-backend"))]
    {
        if app.appliance_cert_entries.is_empty() {
            draw_text(
                buffer,
                width,
                height,
                left,
                section_y + 78,
                "NO TRUSTED LOCAL APPLIANCE CERTIFICATES.",
                TEXT_DIM,
                1,
            );
            return;
        }

        let row_rects = guard_appliance_row_rects(panel, app.appliance_cert_entries.len());
        for (index, rect) in row_rects {
            let selected = app.selected_appliance_cert == Some(index);
            let hovered = app
                .cursor
                .map(|(x, y)| rect.contains(x, y))
                .unwrap_or(false);
            fill_rect(
                buffer,
                width,
                height,
                rect,
                if selected {
                    FIELD_FOCUS
                } else if hovered {
                    PANEL_ALT
                } else {
                    FIELD
                },
            );
            stroke_rect(
                buffer,
                width,
                height,
                rect,
                if selected { BUTTON_ACTIVE } else { BORDER },
            );
            let Some(entry) = app.appliance_cert_entries.get(index) else {
                continue;
            };
            let label = appliance_cert_entry_label(entry);
            let fingerprint = compact_fingerprint(&entry.fingerprint_sha256);
            let created = truncate(&entry.created_at, 24);
            draw_text(
                buffer,
                width,
                height,
                rect.x + 12,
                rect.y + 8,
                &label,
                TEXT,
                1,
            );
            draw_text(
                buffer,
                width,
                height,
                rect.x + 12,
                rect.y + 24,
                &truncate(&format!("{} | {}", fingerprint, created), max_chars),
                TEXT_DIM,
                1,
            );
        }
    }
    #[cfg(not(any(feature = "xilem-shell", feature = "servo-backend")))]
    {
        draw_text(
            buffer,
            width,
            height,
            left,
            section_y + 78,
            "LOCAL APPLIANCE CERTIFICATE STORAGE IS UNAVAILABLE IN THIS BUILD.",
            TEXT_DIM,
            1,
        );
    }
}

pub fn draw_perception_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, BUTTON_BRIGHT);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "PAGE PERCEPTION",
        TEXT,
        1,
    );

    let Some(page) = app.active_tab().and_then(|tab| tab.distilled_page.as_ref()) else {
        draw_text(
            buffer,
            width,
            height,
            44,
            panel.y + 54,
            "NO DISTILLED PAGE YET. DISTILL THE ACTIVE TAB TO BUILD A SEMANTIC MAP.",
            TEXT_DIM,
            1,
        );
        return;
    };

    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    let counts = semantic_counts(page);
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 46,
        &truncate(&page.title, max_chars),
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 70,
        &truncate(page.url.as_str(), max_chars),
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 98,
        &truncate(&page_perception_summary(page), max_chars),
        STATUS_OK,
        1,
    );
    let counts_line = format!(
        "SEMANTICS H={} LINKS={} BUTTONS={} INPUTS={} IMAGES={} TEXT={}",
        counts.headings, counts.links, counts.buttons, counts.inputs, counts.images, counts.text
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 126,
        &truncate(&counts_line, max_chars),
        TEXT_DIM,
        1,
    );
    let source = page
        .metadata
        .get("distillation_backend")
        .or_else(|| page.metadata.get("source"))
        .or_else(|| page.metadata.get("distiller"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 150,
        &truncate(&format!("SOURCE {}", source), max_chars),
        TEXT_DIM,
        1,
    );

    let header_y = panel.y + 190;
    draw_text(buffer, width, height, 44, header_y, "TYPE", TEXT, 1);
    draw_text(buffer, width, height, 132, header_y, "SELECTOR", TEXT, 1);
    draw_text(buffer, width, height, 362, header_y, "TEXT", TEXT, 1);

    let row_start = header_y + 28;
    let max_rows = panel.y.saturating_add(panel.h).saturating_sub(row_start) / 28;
    let selector_chars = 28;
    let text_chars = ((panel.w.saturating_sub(370)) / char_advance(1)) as usize;
    for (index, node) in key_semantic_nodes(page)
        .into_iter()
        .take(max_rows as usize)
        .enumerate()
    {
        let y = row_start + index as u32 * 28;
        let color = match node.node_type {
            NodeType::Heading => TEXT,
            NodeType::Input | NodeType::Button => STATUS_WARN,
            NodeType::Link => BUTTON_BRIGHT,
            NodeType::Image | NodeType::Text => TEXT_DIM,
        };
        draw_text(
            buffer,
            width,
            height,
            44,
            y,
            semantic_node_type_label(&node.node_type),
            color,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            132,
            y,
            &truncate(&node.selector, selector_chars),
            TEXT_DIM,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            362,
            y,
            &truncate(&node.text, text_chars),
            color,
            1,
        );
    }
}

pub fn draw_perf_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_WARN);

    let max_chars = ((panel.w.saturating_sub(40)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "BROWSER PERFORMANCE",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 46,
        &truncate(&format!("LATEST {}", app.perf.summary()), max_chars),
        TEXT_DIM,
        1,
    );

    let slowest = app
        .slowest_perf_event()
        .map(|event| {
            format!(
                "SLOWEST {} {} {}",
                event.phase,
                format_duration(event.duration),
                event.label
            )
        })
        .unwrap_or_else(|| "SLOWEST pending".to_string());
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 72,
        &truncate(&slowest, max_chars),
        STATUS_WARN,
        1,
    );

    let header_y = panel.y + 112;
    draw_text(buffer, width, height, 44, header_y, "PHASE", TEXT, 1);
    draw_text(buffer, width, height, 176, header_y, "TIME", TEXT, 1);
    draw_text(buffer, width, height, 276, header_y, "DETAIL", TEXT, 1);

    if app.perf_events.is_empty() {
        draw_text(
            buffer,
            width,
            height,
            44,
            header_y + 34,
            "NO PERFORMANCE EVENTS YET. OPEN, DISTILL, OR CAPTURE A FRAME.",
            TEXT_DIM,
            1,
        );
        return;
    }

    let row_start = header_y + 34;
    let max_rows = panel.y.saturating_add(panel.h).saturating_sub(row_start) / 24;
    let detail_chars = ((panel.w.saturating_sub(320)) / char_advance(1)) as usize;
    let slowest_event = app.slowest_perf_event();
    for (index, event) in app
        .perf_events
        .iter()
        .rev()
        .take(max_rows as usize)
        .enumerate()
    {
        let y = row_start + index as u32 * 24;
        let color = if slowest_event
            .map(|slow| slow.phase == event.phase && slow.duration == event.duration)
            .unwrap_or(false)
        {
            STATUS_WARN
        } else {
            TEXT_DIM
        };
        draw_text(buffer, width, height, 44, y, event.phase, color, 1);
        draw_text(
            buffer,
            width,
            height,
            176,
            y,
            &format_duration(event.duration),
            color,
            1,
        );
        draw_text(
            buffer,
            width,
            height,
            276,
            y,
            &truncate(&event.label, detail_chars),
            color,
            1,
        );
    }
}

pub fn draw_validation_panel(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    let panel = main_panel_rect(rail_x, height);
    draw_panel_surface(buffer, width, height, panel, STATUS_OK);

    let rows = app.validation_rows();
    let pass_count = app.validation_pass_count();
    let summary = format!("{} OF {} CHECKS COMPLETE", pass_count, rows.len());
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 18,
        "NATIVE BROWSER VALIDATION",
        TEXT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 44,
        &summary,
        if pass_count == rows.len() {
            STATUS_OK
        } else {
            STATUS_WARN
        },
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        44,
        panel.y + 70,
        if app.showcase_report.is_empty() {
            "THIS VIEW TRACKS THE CURRENT WINDOW SESSION, NOT ONLY OPERATOR AUTOMATION."
        } else {
            "BROWSER PROOF IS ACTIVE. RECENT PASSED STEPS ARE LISTED BELOW."
        },
        TEXT_DIM,
        1,
    );

    let mut row_start = panel.y + 112;
    if !app.showcase_report.is_empty() {
        let report_rect = Rect {
            x: 44,
            y: panel.y + 98,
            w: panel.w.saturating_sub(88),
            h: 138,
        };
        fill_rect(buffer, width, height, report_rect, PANEL_DARK);
        stroke_rect(buffer, width, height, report_rect, BORDER);
        draw_text(
            buffer,
            width,
            height,
            report_rect.x + 14,
            report_rect.y + 12,
            proof_report_title(&app.showcase_report),
            TEXT,
            1,
        );
        for (index, line) in app
            .showcase_report
            .iter()
            .filter(|line| !line.starts_with("isolated data dir"))
            .take(6)
            .enumerate()
        {
            let y = report_rect.y + 36 + index as u32 * 16;
            fill_rect(
                buffer,
                width,
                height,
                Rect {
                    x: report_rect.x + 14,
                    y: y + 2,
                    w: 6,
                    h: 6,
                },
                STATUS_OK,
            );
            draw_text(
                buffer,
                width,
                height,
                report_rect.x + 28,
                y,
                &truncate(
                    line,
                    ((report_rect.w.saturating_sub(44)) / char_advance(1)) as usize,
                ),
                TEXT_DIM,
                1,
            );
        }
        row_start = report_rect.y + report_rect.h + 24;
    }

    let max_detail_chars = ((panel.w.saturating_sub(260)) / char_advance(1)) as usize;
    let max_rows = panel.y.saturating_add(panel.h).saturating_sub(row_start) / 38;
    for (index, row) in rows.iter().take(max_rows as usize).enumerate() {
        let y = row_start + index as u32 * 38;
        let color = validation_status_color(row.status);
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: 44,
                y,
                w: 10,
                h: 10,
            },
            color,
        );
        draw_text(
            buffer,
            width,
            height,
            62,
            y - 2,
            validation_status_label(row.status),
            color,
            1,
        );
        draw_text(buffer, width, height, 132, y - 2, row.label, TEXT, 1);
        draw_text(
            buffer,
            width,
            height,
            268,
            y - 2,
            &truncate(&row.detail, max_detail_chars),
            TEXT_DIM,
            1,
        );
    }
}

pub fn draw_field_with_placeholder(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    value: &str,
    placeholder: &str,
    focused: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if focused { FIELD_FOCUS } else { FIELD },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if focused { BUTTON_ACTIVE } else { BORDER },
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x + 1,
            y: rect.y + 1,
            w: rect.w.saturating_sub(2),
            h: 2,
        },
        if focused { BUTTON_ACTIVE } else { BORDER_SOFT },
    );
    let showing_placeholder = value.is_empty() && !placeholder.is_empty() && !focused;
    let visible_source = if showing_placeholder {
        placeholder
    } else {
        value
    };
    let visible = truncate(visible_source, ((rect.w.saturating_sub(24)) / 8) as usize);
    draw_text(
        buffer,
        width,
        height,
        rect.x + 12,
        rect.y + 9,
        &visible,
        if showing_placeholder {
            TEXT_PLACEHOLDER
        } else {
            TEXT
        },
        1,
    );
    if focused {
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: rect.x,
                y: rect.y,
                w: 3,
                h: rect.h,
            },
            BUTTON_ACTIVE,
        );
        let caret_x = rect.x + 12 + text_width(&visible, 1) + 2;
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: caret_x.min(rect.x + rect.w.saturating_sub(8)),
                y: rect.y + 7,
                w: 2,
                h: 16,
            },
            BUTTON_ACTIVE,
        );
    }
}

pub fn proof_report_title(report: &[String]) -> &'static str {
    if report
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("real browsing"))
    {
        "REAL BROWSING"
    } else if report
        .iter()
        .any(|line| line.to_ascii_lowercase().contains("shell interaction"))
    {
        "SHELL SMOKE"
    } else {
        "LAUNCH SHOWCASE"
    }
}

/// Quick-intent presets (Xilem dashboard salvage: the intent-bar preset buttons).
/// Shown in the AI rail's consent band when no consent is pending and the mode
/// allows native intents; clicking runs the intent through the local model brain.
/// Always compiled (label data) so the softbuffer reader shell builds without the
/// `xilem-shell` feature; the click path that runs an intent stays feature-gated.
pub const AI_RAIL_PRESETS: [(&str, &str); 2] = [
    (
        "SUMMARIZE",
        "intent: open example.com and summarize the page",
    ),
    ("DISTILL HERE", "intent: distill the active page into Wake"),
];

/// Map a pilot status to a palette color (Xilem dashboard salvage): green for
/// idle/complete/authorized, amber for consent/blocked, red for failed, cyan for
/// in-progress (reasoning/navigating/distilling/perceiving/planning). Tolerates
/// an optional leading "PILOT " prefix on the status string.
pub fn pilot_status_color(status: &str) -> u32 {
    match status.trim_start_matches("PILOT ").trim() {
        "IDLE" | "COMPLETE" | "CONSENT AUTHORIZED" => STATUS_OK,
        "FAILED" => STATUS_ERROR,
        "BLOCKED" | "AWAITING CONSENT" | "CONSENT DENIED" | "CONSENT RESUMING" => STATUS_WARN,
        _ => STATUS_INFO,
    }
}

pub fn draw_ai_rail(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let rail_x = right_rail_x(width);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: RAIL_WIDTH,
            h: height.saturating_sub(64 + STATUS_BAR_H),
        },
        PANEL_DARK,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: RAIL_WIDTH,
            h: 74,
        },
        PANEL_HEADER,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: 1,
            h: height.saturating_sub(64),
        },
        BORDER,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rail_x,
            y: 64,
            w: 2,
            h: height.saturating_sub(64 + STATUS_BAR_H),
        },
        BUTTON_BRIGHT,
    );
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        86,
        "MAYA SIDECAR",
        TEXT,
        1,
    );
    draw_status_chip(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 108,
            w: 126,
            h: 22,
        },
        &format!(
            "PILOT {}",
            truncate(app.pilot_status.trim_start_matches("PILOT ").trim(), 11)
        ),
        pilot_status_color(&app.pilot_status),
        app.pilot_status != "FAILED",
    );
    let active_url = app
        .active_tab()
        .and_then(|tab| tab.url.as_ref())
        .map(short_url)
        .unwrap_or_else(|| "NO ACTIVE PAGE".to_string());
    let page_state = if app.last_intent.is_empty() {
        app.active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .map(|page| format!("I HAVE DISTILLED '{}'.", page.title))
            .unwrap_or_else(|| format!("TRACKING {}. DISTILL RUNS REAL HTTP FETCH.", active_url))
    } else {
        format!("INTENT: {}", truncate(&app.last_intent, 70))
    };
    let plan_summary = if app.pilot_plan.is_empty() {
        "NEXT: OPEN A PAGE OR ENTER A NATIVE INTENT.".to_string()
    } else {
        format!("PLAN: {}", truncate(&app.pilot_plan.join(" -> "), 72))
    };
    let next_step = if app.last_intent.is_empty() && app.active_tab().is_none() {
        "OPEN A PAGE TO START THE BROWSER LOOP.".to_string()
    } else if app.last_intent.is_empty()
        && app
            .active_tab()
            .and_then(|tab| tab.distilled_page.as_ref())
            .is_none()
    {
        "NEXT: DISTILL THE PAGE INTO WAKE.".to_string()
    } else if app.wake_results.is_empty() {
        plan_summary
    } else {
        format!("RESULT: {}", truncate(&app.pilot_result, 72))
    };

    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        146,
        "ACTIVE CONTEXT",
        TEXT_DIM,
        1,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 164,
            w: RAIL_WIDTH.saturating_sub(40),
            h: 88,
        },
        &page_state,
        false,
    );
    if let Some(run) = app.last_pilot_run.as_ref() {
        // Surface the structured agentic run — the planned steps and the
        // analysis the pilot produced — instead of one truncated plan line.
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            266,
            "PILOT PLAN",
            TEXT_DIM,
            1,
        );
        let mut step_y = 286;
        for (index, step) in run.plan.iter().take(4).enumerate() {
            let detail = truncate(&step.detail, 26);
            let line = if detail.is_empty() {
                format!("{}. {}", index + 1, step.kind.to_uppercase())
            } else {
                format!("{}. {} {}", index + 1, step.kind.to_uppercase(), detail)
            };
            draw_text(
                buffer,
                width,
                height,
                rail_x + 20,
                step_y,
                &line,
                TEXT_SOFT,
                1,
            );
            step_y += 15;
        }
        if let Some(analysis) = run.analysis.iter().find(|line| !line.trim().is_empty()) {
            draw_text(
                buffer,
                width,
                height,
                rail_x + 20,
                step_y + 3,
                &truncate(&format!("> {}", analysis), 40),
                TEXT,
                1,
            );
        }
    } else {
        draw_text(
            buffer,
            width,
            height,
            rail_x + 56,
            266,
            "NEXT STEP",
            TEXT_DIM,
            1,
        );
        draw_chat_bubble(
            buffer,
            width,
            height,
            Rect {
                x: rail_x + 56,
                y: 284,
                w: RAIL_WIDTH.saturating_sub(76),
                h: 66,
            },
            &next_step,
            true,
        );
    }
    draw_text(
        buffer,
        width,
        height,
        rail_x + 20,
        364,
        "SESSION STATUS",
        TEXT_DIM,
        1,
    );
    draw_chat_bubble(
        buffer,
        width,
        height,
        Rect {
            x: rail_x + 20,
            y: 382,
            w: RAIL_WIDTH.saturating_sub(40),
            h: 54,
        },
        &truncate(&app.last_status, 72),
        false,
    );

    if let Some(pending) = app.pending_consent.as_ref() {
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            444,
            "CAPTAIN'S KEY",
            TEXT_DIM,
            1,
        );
        let authorize_hovered = app
            .cursor
            .map(|(x, y)| app.consent_authorize_rect.contains(x, y))
            .unwrap_or(false);
        let deny_hovered = app
            .cursor
            .map(|(x, y)| app.consent_deny_rect.contains(x, y))
            .unwrap_or(false);
        draw_button(
            buffer,
            width,
            height,
            app.consent_authorize_rect,
            "AUTHORIZE",
            authorize_hovered,
            true,
        );
        draw_button(
            buffer,
            width,
            height,
            app.consent_deny_rect,
            "DENY",
            deny_hovered,
            true,
        );
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            496,
            &truncate(&pending.message, 42),
            TEXT_DIM,
            1,
        );
    } else if app.native_intent_allowed() {
        // Quick-intent presets reuse the (free) consent button band.
        draw_text(
            buffer,
            width,
            height,
            rail_x + 20,
            444,
            "QUICK INTENTS",
            TEXT_DIM,
            1,
        );
        for (index, (label, _intent)) in AI_RAIL_PRESETS.iter().enumerate() {
            let rect = if index == 0 {
                app.consent_authorize_rect
            } else {
                app.consent_deny_rect
            };
            let hovered = app
                .cursor
                .map(|(x, y)| rect.contains(x, y))
                .unwrap_or(false);
            draw_button(buffer, width, height, rect, label, hovered, true);
        }
    }

    draw_text(
        buffer,
        width,
        height,
        app.wake_rect.x,
        app.wake_rect.y.saturating_sub(18),
        "ASK OR SEARCH WAKE",
        TEXT_DIM,
        1,
    );
    draw_field_with_placeholder(
        buffer,
        width,
        height,
        app.wake_rect,
        &app.wake_query,
        "ASK OR SEARCH WAKE",
        app.focus == FocusTarget::Wake,
    );
}

pub fn draw_status_bar(buffer: &mut [u32], width: u32, height: u32, app: &BrowserApp) {
    let y = height.saturating_sub(STATUS_BAR_H);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y,
            w: width,
            h: STATUS_BAR_H,
        },
        PANEL_DARK,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: 0,
            y,
            w: width,
            h: 1,
        },
        BORDER,
    );
    let backend = app
        .active_tab()
        .map(|tab| display_backend(app, tab).to_string())
        .unwrap_or_else(|| "NO TAB".to_string());
    let validation_total = app.validation_rows().len();
    let status = format!(
        "MODE {}    AI {}    STORAGE {}    ENGINE {}    RENDER {}    {}    WAKE RESULTS {}    LOG ENTRIES {}    VALIDATION {}/{}",
        app.browser_mode.label(),
        app.ai_status_label(),
        app.storage_status_label(),
        backend,
        app.render_path_label(),
        app.perf.summary(),
        app.wake_results.len(),
        app.recent_logs.len(),
        app.validation_pass_count(),
        validation_total
    );
    let max_status_chars = (width.saturating_sub(172) / char_advance(1)).max(1) as usize;
    draw_status_dot(
        buffer,
        width,
        height,
        14,
        y + 12,
        if app.last_ok { STATUS_OK } else { STATUS_WARN },
    );
    draw_text(
        buffer,
        width,
        height,
        30,
        y + 14,
        &truncate(&status, max_status_chars),
        TEXT_SOFT,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        width.saturating_sub(124),
        y + 14,
        "SERVO EDGE",
        TEXT_DIM,
        1,
    );
}

pub fn draw_panel_surface(buffer: &mut [u32], width: u32, height: u32, rect: Rect, accent: u32) {
    fill_rect(buffer, width, height, rect, PANEL_ALT);
    stroke_rect(buffer, width, height, rect, BORDER);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x + 1,
            y: rect.y + 1,
            w: rect.w.saturating_sub(2),
            h: 38.min(rect.h.saturating_sub(2)),
        },
        PANEL_HEADER,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: 2,
        },
        accent,
    );
}

pub fn draw_status_dot(buffer: &mut [u32], width: u32, height: u32, x: u32, y: u32, color: u32) {
    fill_rect(buffer, width, height, Rect { x, y, w: 7, h: 7 }, color);
    stroke_rect(
        buffer,
        width,
        height,
        Rect {
            x: x.saturating_sub(2),
            y: y.saturating_sub(2),
            w: 11,
            h: 11,
        },
        BORDER_SOFT,
    );
}

pub fn draw_status_chip(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    accent: u32,
    active: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if active { FIELD_FOCUS } else { PANEL },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { accent } else { BORDER },
    );
    draw_status_dot(buffer, width, height, rect.x + 8, rect.y + 8, accent);
    let max_chars = ((rect.w.saturating_sub(28)) / char_advance(1)) as usize;
    draw_text(
        buffer,
        width,
        height,
        rect.x + 24,
        rect.y + 8,
        &truncate(label, max_chars),
        if active { TEXT } else { TEXT_DIM },
        1,
    );
}

pub fn draw_text_centered(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    color: u32,
    scale: u32,
) {
    let scale = scale.max(1);
    let max_chars = ((rect.w.saturating_sub(8)) / char_advance(scale)) as usize;
    let visible = truncate(label, max_chars);
    let text_w = text_width(&visible, scale);
    let glyph_h = 7 * scale;
    let x = rect.x + rect.w.saturating_sub(text_w) / 2;
    let y = rect.y + rect.h.saturating_sub(glyph_h) / 2;
    draw_text(buffer, width, height, x, y, &visible, color, scale);
}

pub fn draw_metric_card(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    value: &str,
    hint: &str,
    accent: u32,
) {
    fill_rect(buffer, width, height, rect, PANEL_HEADER);
    stroke_rect(buffer, width, height, rect, BORDER);
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: 2,
        },
        accent,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: 2,
            h: rect.h,
        },
        accent,
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 18,
        rect.y + 14,
        label,
        TEXT_DIM,
        1,
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 18,
        rect.y + 34,
        &truncate(value, ((rect.w.saturating_sub(36)) / 13) as usize),
        accent,
        2,
    );
    draw_text(
        buffer,
        width,
        height,
        rect.x + 18,
        rect.y + 62,
        hint,
        TEXT_DIM,
        1,
    );
}

pub fn draw_pill(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    active: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if active { FIELD_FOCUS } else { PANEL_ALT },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if active { BUTTON_BRIGHT } else { BORDER },
    );
    if active {
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: 2,
            },
            BUTTON_BRIGHT,
        );
    }
    draw_text_centered(
        buffer,
        width,
        height,
        rect,
        label,
        if active { TEXT } else { TEXT_DIM },
        1,
    );
}

pub fn draw_chat_bubble(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    text: &str,
    user: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if user { FIELD_FOCUS } else { PANEL_ALT },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if user { BUTTON_BRIGHT } else { BORDER_SOFT },
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: 2,
        },
        if user { BUTTON_BRIGHT } else { BORDER_SOFT },
    );
    let color = if user { TEXT } else { TEXT_SOFT };
    let max = ((rect.w.saturating_sub(24)) / 8) as usize;
    for (index, line) in wrap_text(text, max).iter().take(4).enumerate() {
        draw_text(
            buffer,
            width,
            height,
            rect.x + 12,
            rect.y + 14 + index as u32 * 18,
            line,
            color,
            1,
        );
    }
}

pub fn draw_button(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    rect: Rect,
    label: &str,
    hovered: bool,
    enabled: bool,
) {
    fill_rect(
        buffer,
        width,
        height,
        rect,
        if !enabled {
            BUTTON_DISABLED
        } else if hovered {
            BUTTON_HOVER
        } else {
            PANEL_ALT
        },
    );
    stroke_rect(
        buffer,
        width,
        height,
        rect,
        if enabled { BORDER } else { PANEL },
    );
    if enabled {
        fill_rect(
            buffer,
            width,
            height,
            Rect {
                x: rect.x,
                y: rect.y,
                w: rect.w,
                h: 2,
            },
            if hovered { BUTTON_BRIGHT } else { BORDER_SOFT },
        );
    }
    draw_text_centered(
        buffer,
        width,
        height,
        rect,
        label,
        if enabled { TEXT } else { TEXT_DIM },
        1,
    );
}

pub fn right_rail_x(width: u32) -> u32 {
    let min_main_width = 420;
    if width >= min_main_width + RAIL_WIDTH {
        width.saturating_sub(RAIL_WIDTH)
    } else {
        width
    }
}

pub fn main_panel_rect(rail_x: u32, height: u32) -> Rect {
    let top = METRIC_Y + METRIC_H + 14;
    let bottom_limit = height.saturating_sub(STATUS_BAR_H + 12);
    Rect {
        x: 24,
        y: top,
        w: rail_x.saturating_sub(48),
        h: bottom_limit.saturating_sub(top),
    }
}

pub fn browser_viewport_rect(panel: Rect) -> Rect {
    let header_h = panel.h.min(126);
    let bottom_pad = if panel.h > header_h { 16 } else { 0 };
    Rect {
        x: panel.x + 18,
        y: panel.y + header_h,
        w: panel.w.saturating_sub(36),
        h: panel.h.saturating_sub(header_h + bottom_pad),
    }
}

pub struct SemanticCounts {
    pub headings: usize,
    pub links: usize,
    pub buttons: usize,
    pub inputs: usize,
    pub images: usize,
    pub text: usize,
}

pub fn semantic_counts(page: &sextant_engine::DistilledPage) -> SemanticCounts {
    let mut counts = SemanticCounts {
        headings: 0,
        links: 0,
        buttons: 0,
        inputs: 0,
        images: 0,
        text: 0,
    };
    for node in &page.semantic_map {
        match node.node_type {
            NodeType::Heading => counts.headings += 1,
            NodeType::Link => counts.links += 1,
            NodeType::Input => counts.inputs += 1,
            NodeType::Image => counts.images += 1,
            NodeType::Text => counts.text += 1,
            NodeType::Button => counts.buttons += 1,
        }
    }
    counts
}

pub fn page_perception_summary(page: &sextant_engine::DistilledPage) -> String {
    let counts = semantic_counts(page);
    let kind = if counts.inputs > 0 {
        "interactive form page"
    } else if counts.links >= 20 {
        "navigation-rich reference page"
    } else if counts.headings >= 3 && counts.text >= 8 {
        "structured article page"
    } else if counts.images > counts.text && counts.images > 0 {
        "media-heavy page"
    } else if counts.links > 0 {
        "simple linked document"
    } else {
        "simple document"
    };
    format!(
        "PERCEPTION {} with {} semantic node(s), {} link(s), {} input(s), {} heading(s)",
        kind,
        page.semantic_map.len(),
        counts.links,
        counts.inputs,
        counts.headings
    )
}

pub fn key_semantic_nodes(
    page: &sextant_engine::DistilledPage,
) -> Vec<&sextant_engine::SemanticNode> {
    let mut nodes = page
        .semantic_map
        .iter()
        .filter(|node| !node.text.trim().is_empty() || !node.selector.trim().is_empty())
        .collect::<Vec<_>>();
    nodes.sort_by_key(|node| match node.node_type {
        NodeType::Heading => 0,
        NodeType::Input => 1,
        NodeType::Button => 2,
        NodeType::Link => 3,
        NodeType::Image => 4,
        NodeType::Text => 5,
    });
    nodes
}

pub fn semantic_node_type_label(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::Heading => "HEAD",
        NodeType::Link => "LINK",
        NodeType::Button => "BUTTON",
        NodeType::Input => "INPUT",
        NodeType::Image => "IMAGE",
        NodeType::Text => "TEXT",
    }
}

pub fn guard_report_lines(
    app: &BrowserApp,
    target_url: Option<&Url>,
    _page: Option<&sextant_engine::DistilledPage>,
) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("guard persona: {}", app.persona_id));
    #[cfg(feature = "xilem-shell")]
    {
        lines.push(format!("guard policy: {}", app.guard_policy_source));
        lines.push(format!(
            "guard rules: persona={} global_blacklist={}",
            app.guard_firewall.persona_rules(&app.persona_id).len(),
            app.guard_firewall.global_blacklist().len()
        ));
        let airgap = SextantAirGap::new();
        let network = if airgap.check_network_allowed() {
            "ONLINE network allowed"
        } else {
            "ISOLATED network blocked"
        };
        lines.push(format!("airgap: {:?} | {}", airgap.get_status(), network));

        if let Some(url) = target_url {
            let (action, reason) = app.guard_firewall.check_access(&app.persona_id, url);
            lines.push(format!(
                "firewall: {} {} | {}",
                firewall_action_label(&action),
                short_url(url),
                reason
            ));
        } else {
            lines.push("firewall: waiting for an active target URL".to_string());
        }

        let masker = PrivacyMasker::new();
        let sample = _page
            .map(|page| page.content.as_str())
            .unwrap_or("Contact Paul at paul@example.com or 555-123-4567.");
        let redacted = masker.redact_text(sample, &PrivacyLevel::Standard);
        let redacted_changed = redacted != sample;
        lines.push(format!(
            "privacy: STANDARD redaction {}",
            if redacted_changed {
                "active"
            } else {
                "no pii found"
            }
        ));
        lines.push(format!("privacy sample: {}", truncate(&redacted, 96)));
    }
    #[cfg(not(feature = "xilem-shell"))]
    {
        if let Some(url) = target_url {
            lines.push(format!("firewall: reader lane observes {}", short_url(url)));
        } else {
            lines.push("firewall: reader lane waiting for target URL".to_string());
        }
        lines.push("airgap: reader lane, guard crates disabled".to_string());
        lines.push("privacy: reader lane, redaction unavailable".to_string());
    }
    lines
}

#[cfg(feature = "xilem-shell")]
pub fn firewall_action_label(action: &FirewallAction) -> &'static str {
    match action {
        FirewallAction::Allow => "ALLOW",
        FirewallAction::Block => "BLOCK",
        FirewallAction::Audit => "AUDIT",
        FirewallAction::Isolate => "ISOLATE",
    }
}

pub fn distillation_label(page: &sextant_engine::DistilledPage) -> String {
    page.metadata
        .get("distillation_backend")
        .or_else(|| page.metadata.get("source"))
        .or_else(|| page.metadata.get("distiller"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}

pub fn status_label(status: &LogStatus) -> &'static str {
    match status {
        LogStatus::Success => "OK",
        LogStatus::Failure(_) => "FAIL",
        LogStatus::Aborted => "ABORT",
        LogStatus::AwaitingConsent => "CONSENT",
    }
}

pub fn validation_status_label(status: ValidationStatus) -> &'static str {
    match status {
        ValidationStatus::Pass => "PASS",
        ValidationStatus::Waiting => "WAIT",
        ValidationStatus::Attention => "CHECK",
    }
}

pub fn validation_status_color(status: ValidationStatus) -> u32 {
    match status {
        ValidationStatus::Pass => STATUS_OK,
        ValidationStatus::Waiting => TEXT_DIM,
        ValidationStatus::Attention => STATUS_WARN,
    }
}

pub fn wrap_text(value: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(12);
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        if !current.is_empty() && current.len() + word.len() + 1 > max_chars {
            lines.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

pub fn truncate(value: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return ".".repeat(max_chars);
    }
    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    out.push_str("...");
    out
}

pub fn fill_rect(buffer: &mut [u32], width: u32, height: u32, rect: Rect, color: u32) {
    // Clamp the span once (x0 <= max_x always holds) and fill each row with a
    // slice `fill` so the compiler can emit a memset instead of a per-pixel
    // store — this is the software-render hot path (panels, backgrounds, and
    // every glyph blit go through here).
    let x0 = rect.x.min(width) as usize;
    let max_x = rect.x.saturating_add(rect.w).min(width) as usize;
    let max_y = rect.y.saturating_add(rect.h).min(height);
    for y in rect.y.min(height)..max_y {
        let row = y as usize * width as usize;
        buffer[row + x0..row + max_x].fill(color);
    }
}

pub fn stroke_rect(buffer: &mut [u32], width: u32, height: u32, rect: Rect, color: u32) {
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: 1,
        },
        color,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y.saturating_add(rect.h.saturating_sub(1)),
            w: rect.w,
            h: 1,
        },
        color,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x,
            y: rect.y,
            w: 1,
            h: rect.h,
        },
        color,
    );
    fill_rect(
        buffer,
        width,
        height,
        Rect {
            x: rect.x.saturating_add(rect.w.saturating_sub(1)),
            y: rect.y,
            w: 1,
            h: rect.h,
        },
        color,
    );
}

pub fn draw_text(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    mut x: u32,
    y: u32,
    text: &str,
    color: u32,
    scale: u32,
) {
    let scale = scale.max(1);
    for ch in text.chars() {
        if ch == '\n' {
            x = 0;
            continue;
        }
        draw_char(buffer, width, height, x, y, ch, color, scale);
        x += char_advance(scale);
    }
}

pub fn draw_text_clipped(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    clip: Rect,
    mut x: u32,
    y: i32,
    text: &str,
    color: u32,
    scale: u32,
) {
    let scale = scale.max(1);
    for ch in text.chars() {
        draw_char_clipped(buffer, width, height, clip, x, y, ch, color, scale);
        x += char_advance(scale);
        if x >= clip.x.saturating_add(clip.w) {
            break;
        }
    }
}

pub fn text_width(text: &str, scale: u32) -> u32 {
    let chars = text.chars().count() as u32;
    if chars == 0 {
        0
    } else {
        chars * char_advance(scale)
    }
}

pub fn char_advance(scale: u32) -> u32 {
    (GLYPH_W + GLYPH_GAP) * scale.max(1)
}

pub fn draw_char(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    x: u32,
    y: u32,
    ch: char,
    color: u32,
    scale: u32,
) {
    // Write each lit glyph bit's scale×scale block straight into the buffer
    // instead of calling fill_rect per bit. The clamped span is identical to
    // fill_rect's ([x0,max_x) × [y0,max_y)); we just hoist the per-row y-range
    // and skip the function call — text is the hottest thing the renderer does.
    let glyph = glyph(ch);
    let stride = width as usize;
    for (row, &bits) in glyph.iter().enumerate() {
        if bits == 0 {
            continue;
        }
        let py = y + row as u32 * scale;
        let y_start = py.min(height);
        let y_end = py.saturating_add(scale).min(height);
        for col in 0..5u32 {
            if bits & (1 << (4 - col)) == 0 {
                continue;
            }
            let px = x + col * scale;
            let x_start = px.min(width) as usize;
            let x_end = px.saturating_add(scale).min(width) as usize;
            for yy in y_start..y_end {
                let base = yy as usize * stride;
                buffer[base + x_start..base + x_end].fill(color);
            }
        }
    }
}

pub fn draw_char_clipped(
    buffer: &mut [u32],
    width: u32,
    height: u32,
    clip: Rect,
    x: u32,
    y: i32,
    ch: char,
    color: u32,
    scale: u32,
) {
    // Same inline fill as draw_char, keeping the existing clip semantics: only
    // the block's top-left pixel is tested against `clip` (the scale×scale block
    // itself is clamped to the buffer, not to clip — unchanged behavior).
    let glyph = glyph(ch);
    let stride = width as usize;
    for (row, &bits) in glyph.iter().enumerate() {
        if bits == 0 {
            continue;
        }
        let pixel_y = y + row as i32 * scale as i32;
        if pixel_y < clip.y as i32 || pixel_y >= (clip.y + clip.h) as i32 {
            continue;
        }
        let py = pixel_y as u32;
        let y_start = py.min(height);
        let y_end = py.saturating_add(scale).min(height);
        for col in 0..5u32 {
            if bits & (1 << (4 - col)) == 0 {
                continue;
            }
            let pixel_x = x + col * scale;
            if pixel_x < clip.x || pixel_x >= clip.x.saturating_add(clip.w) {
                continue;
            }
            let x_start = pixel_x.min(width) as usize;
            let x_end = pixel_x.saturating_add(scale).min(width) as usize;
            for yy in y_start..y_end {
                let base = yy as usize * stride;
                buffer[base + x_start..base + x_end].fill(color);
            }
        }
    }
}

pub fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0f, 0x10, 0x10, 0x13, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0e],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
        ':' => [0x00, 0x0c, 0x0c, 0x00, 0x0c, 0x0c, 0x00],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '<' => [0x01, 0x02, 0x04, 0x08, 0x04, 0x02, 0x01],
        '>' => [0x10, 0x08, 0x04, 0x02, 0x04, 0x08, 0x10],
        '+' => [0x00, 0x04, 0x04, 0x1f, 0x04, 0x04, 0x00],
        '-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        '_' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1f],
        '?' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
        '&' => [0x0c, 0x12, 0x14, 0x08, 0x15, 0x12, 0x0d],
        '=' => [0x00, 0x00, 0x1f, 0x00, 0x1f, 0x00, 0x00],
        '\'' => [0x0c, 0x04, 0x08, 0x00, 0x00, 0x00, 0x00],
        '"' => [0x0a, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00],
        '|' => [0x04, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        _ => [0x1f, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04],
    }
}
