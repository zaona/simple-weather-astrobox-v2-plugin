//! 背景图页签：传输状态、图片处理滑块、12 个天气编号的背景图管理。
//!
//! 两套版式，按宿主渲染区宽度分派（见 `UiState::is_compact_layout`）：
//!
//! - 移动端（<640px）：单列列表，右侧一颗切换/加号按钮
//! - 桌面端（≥640px）：自适应列网格，每张一个竖版卡片
//!
//! 交互对齐安卓 `BackgroundImageItem`，并按端上交互做了调整：
//!
//! - 没有图 → 点整张卡片/整行 → 拉起系统图片选择器导入
//! - 已有图 → 点图 → 打开灯箱预览；卡片右上角的切换按钮 → 换一张
//! - 删除放在灯箱里，避免卡片上堆两颗按钮
//!
//! 滑块用宿主的 `SLIDER`（Radix Themes Slider），只传 `default-value`：宿主只挂了
//! `onValueCommit`，一旦传 `value` 就变成受控模式，拖动过程中 thumb 不会跟随，
//! 看起来就是滑不动。数值由上方文案在松手提交后带出。

use super::event_handler::*;
use super::icons;
use super::state::*;
use crate::astrobox::psys_host_v4::ui;

/// 滑块量程，与安卓 `valueRange = 0f..100f` 一致
const SLIDER_MAX: u32 = 100;
const ROW_THUMB_SIZE: u32 = 40;
const CARD_THUMB_WIDTH: u32 = 168;
const CARD_THUMB_HEIGHT: u32 = 214;
const LIGHTBOX_WIDTH: u32 = 300;
const LIGHTBOX_HEIGHT: u32 = 357;

/// 传输状态那行的固定高度（单行 13px 文字）
const STATUS_LINE_HEIGHT: u32 = 18;

const CARD_BG: &str = "#1E1E1F";
const CARD_INNER_BG: &str = "#2A2A2A";
const CARD_ITEM_BG: &str = "#232325";
const MUTED: &str = "#8E8E93";
const FAINT: &str = "#48484A";
const ACCENT: &str = "#0090FF";
const DANGER: &str = "#FF453A";

pub fn build_bg_tab(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(10);

    if state.bg_guide {
        return build_guide();
    }
    if let Some(code) = state.bg_lightbox.clone() {
        return build_lightbox(state, &code);
    }

    root.child(build_transfer_card())
        .child(build_preset_card(state))
        .child(build_section_title("图片处理"))
        .child(build_edit_card(state))
        .child(build_section_title("背景图"))
        .child(build_code_list_card(state))
}

/// 说明 / 导入 / 导出：独立成一个动作容器
///
/// 和顶部那张传输状态卡刻意区分：状态卡是纯信息底色的两行文字，这里是带图标的
/// 可点条目，用淡蓝底 + 强调色图标，一眼能看出是操作而不是状态。
/// 窄屏排成整行列表，宽屏才排成三列卡片。
fn build_preset_card(state: &UiState) -> ui::Element {
    let compact = state.is_compact_layout();
    let has_images = !state.bg_codes.is_empty();

    let guide = action_entry(
        icons::info_svg(),
        "说明",
        BG_GUIDE_EVENT,
        ACCENT,
        compact,
    );
    let import = action_entry(
        icons::download_simple_svg(),
        "导入预设包",
        BG_IMPORT_EVENT,
        ACCENT,
        compact,
    );
    let export = action_entry(
        icons::upload_simple_svg(),
        "导出预设包",
        BG_EXPORT_EVENT,
        if has_images { ACCENT } else { "#636366" },
        compact,
    );

    let card = build_card().padding(8).gap(6);
    if compact {
        return card.child(guide).child(import).child(export);
    }

    card.child(
        ui::Element::new(ui::ElementType::Grid, None)
            .width_full()
            .prop("columns", "repeat(3, 1fr)")
            .prop("gap", "8px")
            .child(guide)
            .child(import)
            .child(export),
    )
}

/// 窄屏是整行（图标在左、文字在右），宽屏是竖排小卡片
fn action_entry(
    icon_svg: String,
    label: &str,
    event_id: &str,
    color: &str,
    compact: bool,
) -> ui::Element {
    let enabled = color == ACCENT;

    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(if compact { 18 } else { 20 })
        .height(if compact { 18 } else { 20 })
        .text_color(color);

    let text = ui::Element::new(ui::ElementType::Span, Some(label))
        .size(if compact { 14 } else { 12 })
        .text_color(color);

    let button = ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(12)
        .bg(if enabled { "#0090FF1F" } else { "#FFFFFF0A" });

    if compact {
        return button
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .align_center()
            .width_full()
            .gap(10)
            .padding_top(11)
            .padding_bottom(11)
            .padding_left(12)
            .padding_right(12)
            .child(icon)
            .child(text);
    }

    button
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .align_center()
        .justify_center()
        .gap(6)
        .padding_top(12)
        .padding_bottom(12)
        .padding_left(6)
        .padding_right(6)
        .child(icon)
        .child(text)
}

/// 使用说明。写的是插件端真实行为，不照搬安卓端那套（端上选图 + 与快应用两套独立存储）。
fn build_guide() -> ui::Element {
    let sections: [(&str, &str); 9] = [
        (
            "两种图片来源",
            "① 端上选图：点没有背景图的卡片（或移动端整行）打开系统文件选择器，支持 PNG / JPG / WEBP，单张上限 12 MB。② 手机端推图：手机「自定义背景图」页点同步，覆盖式传到本插件。两者共用同一份存储，先到先生效。",
        ),
        (
            "已配置的样子",
            "有图的卡片/行右上角是 ✕ 删除，点图本身打开大图灯箱。没有图时整块可点，点一下就是导入。桌面端排成多列卡片，移动端排成单列列表，行为完全一致。",
        ),
        (
            "删除即恢复默认",
            "✕ 会连同原始图片一起删除（删除前二次确认，不可撤销），删完这一格回到未配置状态。注意：删除只影响本插件，手机端和手环快应用各自的副本不受影响。",
        ),
        (
            "压暗与模糊",
            "两个滑块范围都是 0-100，且是全局参数——改一次会按新参数重算所有已配置的背景图，右上状态卡会显示重算进度。",
        ),
        (
            "画质固定",
            "出图统一按 RGB_565 量化后存 PNG，不提供画质滑块（安卓端的「画质」参数导入时会读入但不参与处理），这样传输体积和渲染开销都是稳定的。",
        ),
        (
            "原件与成品",
            "每张图存两份：bg/source-* 是你选的原始图片，bg/custom-bg-* 是按当前参数算出来的成品。滑块永远从原件重算，所以参数往回调不会把图越调越糊。",
        ),
        (
            "手机端推图的生效时机",
            "本插件侧收到就立刻可见；但手环快应用那侧的壁纸需要退出快应用再重新进入才刷新，若覆盖的是已有背景图，可能还要重启手环清图片缓存。",
        ),
        (
            "导入 / 导出预设包",
            "预设包是 .swbg 文件（本质是 ZIP：manifest.json + images/），导出走系统保存对话框、导入走系统文件选择器。包里存的是原始图片和参数，所以来回导不会掉画质；本插件导的包手机端能直接导入，反之亦然。",
        ),
        (
            "为什么没有分享按钮",
            "宿主给插件的接口里没有系统分享能力（只有保存文件到本地），所以分享被做成了「导出预设包」：存到本地后自行发送。插件与手机端共享同一份预设包格式。",
        ),
    ];

    let mut card = build_card().child(
        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .align_center()
            .width_full()
            .gap(10)
            .child(
                ui::Element::new(ui::ElementType::Button, None)
                    .without_default_styles()
                    .on(ui::Event::Click, BG_CLOSE_GUIDE_EVENT)
                    .radius(999)
                    .width(32)
                    .height(32)
                    .bg(CARD_INNER_BG)
                    .flex()
                    .align_center()
                    .justify_center()
                    .child(
                        ui::Element::new(ui::ElementType::Svg, Some(&icons::back_arrow_svg()))
                            .width(18)
                            .height(18)
                            .text_color("#FFFFFF"),
                    ),
            )
            .child(ui::Element::new(ui::ElementType::P, Some("使用说明")).size(15)),
    );

    for (title, body) in sections {
        card = card
            .child(ui::Element::new(ui::ElementType::P, Some(title)).size(14))
            .child(
                ui::Element::new(ui::ElementType::P, Some(body))
                    .size(12)
                    .text_color(MUTED),
            );
    }

    card.child(build_small_button(
        "打开完整文档",
        OPEN_HELP_DOC_EVENT,
        CARD_INNER_BG,
        ACCENT,
    ))
}

/// 灯箱：整屏看原图，换图 / 删除 / 返回都在这里
fn build_lightbox(state: &UiState, code: &str) -> ui::Element {
    let title = format!("{} · {}", code, crate::bg::store::label_of(code));

    let mut card = build_card().child(
        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .align_center()
            .width_full()
            .gap(10)
            .child(
                ui::Element::new(ui::ElementType::Button, None)
                    .without_default_styles()
                    .on(ui::Event::Click, BG_CLOSE_LIGHTBOX_EVENT)
                    .radius(999)
                    .width(32)
                    .height(32)
                    .bg(CARD_INNER_BG)
                    .flex()
                    .align_center()
                    .justify_center()
                    .child(
                        ui::Element::new(ui::ElementType::Svg, Some(&icons::back_arrow_svg()))
                            .width(18)
                            .height(18)
                            .text_color("#FFFFFF"),
                    ),
            )
            .child(ui::Element::new(ui::ElementType::P, Some(&title)).size(15)),
    );

    if state.bg_preview_uri.is_empty() {
        return card.child(
            ui::Element::new(ui::ElementType::P, Some("图片读取失败，请重新导入"))
                .size(13)
                .text_color(DANGER),
        );
    }

    card = card
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .align_center()
        .child(
            ui::Element::new(ui::ElementType::Image, Some(&state.bg_preview_uri))
                .radius(14)
                .width(LIGHTBOX_WIDTH)
                .height(LIGHTBOX_HEIGHT),
        );

    let remove = build_small_button(
        "删除并恢复默认",
        &format!("{}{}", BG_DELETE_PREFIX, code),
        "#FF453A1F",
        DANGER,
    );
    let back = build_small_button("返回列表", BG_CLOSE_LIGHTBOX_EVENT, CARD_INNER_BG, "#FFFFFF");

    if state.is_compact_layout() {
        return card.child(back).child(remove);
    }
    card.flex_direction(ui::FlexDirection::Row)
        .justify_center()
        .child(back)
        .child(remove)
}

/// 传输状态：文案对齐手环端 `image-service.js` 的 message
fn build_transfer_card() -> ui::Element {
    let progress = crate::bg::session::progress();
    let receiving = progress.phase == crate::bg::session::Phase::Receiving;

    let percent = if progress.total_chunks > 0 {
        progress.received.saturating_mul(100) / progress.total_chunks
    } else {
        0
    };
    // 重算进度不展示：分帧跑完很快，卡片高度保持固定即可
    let detail = if receiving {
        format!(
            "{}  {}/{}  {}%",
            progress.message, progress.received, progress.total_chunks, percent
        )
    } else if progress.message.trim().is_empty() {
        "等待传输".to_string()
    } else {
        progress.message.clone()
    };

    let mut row = build_card().child(
        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Column)
            .width_full()
            .gap(2)
            .child(ui::Element::new(ui::ElementType::P, Some("背景图传输")).size(15))
            // 固定单行高度：文案在"等待/接收/重算/完成"之间切换时不撑高卡片，
            // 否则下面的"已配置 N/12"会被顶得上下跳
            .child(
                ui::Element::new(ui::ElementType::P, Some(&detail))
                    .size(13)
                    .height(STATUS_LINE_HEIGHT)
                    .text_color(if receiving { ACCENT } else { MUTED }),
            ),
    );

    if receiving {
        row = row.child(build_small_button(
            "取消传输",
            BG_CANCEL_EVENT,
            "#FF453A26",
            DANGER,
        ));
    }
    row
}

/// 图片处理：压暗 + 模糊，滑块上方左侧标签、右侧数值
fn build_edit_card(state: &UiState) -> ui::Element {
    let darken = build_slider_row(
        icons::darken_svg(),
        "压暗",
        state.bg_darken,
        BG_DARKEN_SLIDER_EVENT,
    );
    let blur = build_slider_row(
        icons::blur_svg(),
        "模糊",
        state.bg_blur,
        BG_BLUR_SLIDER_EVENT,
    );

    let card = build_card();
    if state.is_compact_layout() {
        return card.child(darken).child(blur);
    }

    // 桌面：两条滑块并排
    card.flex_direction(ui::FlexDirection::Row)
        .align_center()
        .gap(24)
        .child(darken.flex_grow(1.0))
        .child(blur.flex_grow(1.0))
}

fn build_slider_row(icon_svg: String, label: &str, value: u32, event_id: &str) -> ui::Element {
    let value_text = value.to_string();
    let max_prop = SLIDER_MAX.to_string();
    let value_prop = value.to_string();

    let header = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(8)
        .child(
            ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
                .width(18)
                .height(18)
                .text_color(MUTED),
        )
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(14))
        .child(build_spacer())
        .child(
            ui::Element::new(ui::ElementType::P, Some(&value_text))
                .size(14)
                .text_color(ACCENT),
        );

    let slider = ui::Element::new(ui::ElementType::Slider, None)
        .width_full()
        .prop("min", "0")
        .prop("max", &max_prop)
        .prop("step", "1")
        .prop("default-value", &value_prop)
        .prop("color", "accent")
        .prop("variant", "soft")
        .prop("radius", "full")
        .prop("size", "2")
        .on(ui::Event::Change, event_id);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(6)
        .child(header)
        .child(slider)
}

/// 已配置 N/12 与删除按钮，下方按版式排列表或卡片
fn build_code_list_card(state: &UiState) -> ui::Element {
    let mut header = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .child(
            ui::Element::new(ui::ElementType::P, Some(&format!(
                "已配置 {}/12 张",
                state.bg_codes.len()
            )))
            .size(14),
        )
        .child(build_spacer());

    if !state.bg_codes.is_empty() {
        header = header.child(build_small_button(
            "删除全部",
            BG_CLEAR_ALL_EVENT,
            "#FF453A1F",
            DANGER,
        ));
    }

    if state.is_compact_layout() {
        let mut card = build_card().padding_top(4).padding_bottom(4).child(header);
        for (code, label) in crate::bg::protocol::WEATHER_BG_CODES.iter() {
            card = card.child(build_code_row(code, label, state));
        }
        return card;
    }

    // 桌面：滚动区 + 自适应列网格
    let mut grid = ui::Element::new(ui::ElementType::Grid, None)
        .width_full()
        .prop(
            "columns",
            &format!("repeat(auto-fill, minmax({}px, 1fr))", crate::ui::state::CARD_MIN_COLUMN),
        )
        .prop("gap", &format!("{}px", crate::ui::state::CARD_COLUMN_GAP));
    for (code, label) in crate::bg::protocol::WEATHER_BG_CODES.iter() {
        grid = grid.child(build_code_card(code, label, state));
    }

    build_card().child(header).child(grid)
}

/// 桌面卡片：竖版预览图在上、天气文字在下，右上角是切换按钮。
/// 没有图 → 整张卡片点一下导入；有图 → 点图开灯箱。
fn build_code_card(code: &str, label: &str, state: &UiState) -> ui::Element {
    let has_custom = state.bg_codes.iter().any(|saved| saved == code);

    let preview = match state.bg_thumbs.get(code).map(String::as_str) {
        Some(uri) => {
            ui::Element::new(ui::ElementType::Image, Some(uri))
                .width(CARD_THUMB_WIDTH)
                .height(CARD_THUMB_HEIGHT)
                .radius(10)
        }
        None => ui::Element::new(ui::ElementType::Div, None)
            .width(CARD_THUMB_WIDTH)
            .height(CARD_THUMB_HEIGHT)
            .radius(10)
            .bg(CARD_INNER_BG)
            .flex()
            .align_center()
            .justify_center()
            .child(
                ui::Element::new(ui::ElementType::Svg, Some(&icons::image_svg()))
                    .width(28)
                    .height(28)
                    .text_color(FAINT),
            ),
    };

    // 右上角动作按钮：没图是加号，有图是叉号
    let thumb = ui::Element::new(ui::ElementType::Div, None)
        .relative()
        .width(CARD_THUMB_WIDTH)
        .height(CARD_THUMB_HEIGHT)
        .child(preview)
        .child(
            ui::Element::new(ui::ElementType::Div, None)
                .absolute()
                .top(6)
                .right(6)
                .child(if has_custom {
                    build_icon_button(
                        icons::x_svg(),
                        &format!("{}{}", BG_DELETE_PREFIX, code),
                        "#1C1C1EBF",
                        DANGER,
                    )
                } else {
                    build_icon_button(
                        icons::plus_svg(),
                        &format!("{}{}", BG_PICK_PREFIX, code),
                        "#1C1C1EBF",
                        ACCENT,
                    )
                }),
        );

    let mut card = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .align_center()
        .width_full()
        .bg(CARD_ITEM_BG)
        .radius(14)
        .padding(10)
        .gap(10)
        .child(thumb)
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(15));

    if has_custom {
        let file_name = crate::bg::store::label_of(code);
        if !file_name.trim().is_empty() {
            card = card.child(
                ui::Element::new(ui::ElementType::P, Some(&file_name))
                    .size(12)
                    .padding_top(4)
                    .padding_bottom(4)
                    .text_color(MUTED),
            );
        }
        card.on(ui::Event::Click, &format!("{}{}", BG_LIGHTBOX_PREFIX, code))
    } else {
        // 没有图：整张卡片就是导入入口
        card.on(ui::Event::Click, &format!("{}{}", BG_PICK_PREFIX, code))
    }
}

/// 列表行：对齐安卓 `BackgroundImageItem`——缩略图 + 两行文字 + 右侧一颗图标
///
/// 没有图 → 整行点一下导入；已有图 → 点行开灯箱，✕ 删除。
fn build_code_row(code: &str, label: &str, state: &UiState) -> ui::Element {
    let has_custom = state.bg_codes.iter().any(|saved| saved == code);

    let thumb = match state.bg_thumbs.get(code).map(String::as_str) {
        Some(uri) => {
            ui::Element::new(ui::ElementType::Image, Some(uri))
                .width(ROW_THUMB_SIZE)
                .height(ROW_THUMB_SIZE)
                .radius(8)
        }
        None => ui::Element::new(ui::ElementType::Div, None)
            .width(ROW_THUMB_SIZE)
            .height(ROW_THUMB_SIZE)
            .radius(8)
            .bg(CARD_INNER_BG)
            .flex()
            .align_center()
            .justify_center()
            .child(
                ui::Element::new(ui::ElementType::Svg, Some(&icons::image_svg()))
                    .width(20)
                    .height(20)
                    .text_color(FAINT),
            ),
    };

    // 两行文字：天气名 + 文件名（没图时不显示第二行）
    let mut text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .flex_grow(1.0)
        .gap(2)
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(15));
    if has_custom {
        let file_name = crate::bg::store::label_of(code);
        if !file_name.trim().is_empty() {
            text_col = text_col.child(
                ui::Element::new(ui::ElementType::P, Some(&file_name))
                    .size(12)
                    .text_color(MUTED),
            );
        }
    }

    let mut row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .padding_top(8)
        .padding_bottom(8)
        .gap(12)
        .child(thumb)
        .child(text_col);

    // 右侧动作按钮：没图是加号，有图是叉号，与安卓一致
    row = row.child(if has_custom {
        build_icon_button(
            icons::x_svg(),
            &format!("{}{}", BG_DELETE_PREFIX, code),
            "#FF453A1F",
            DANGER,
        )
    } else {
        build_icon_button(
            icons::plus_svg(),
            &format!("{}{}", BG_PICK_PREFIX, code),
            CARD_INNER_BG,
            ACCENT,
        )
    });

    // 有图点行看大图，没图点行直接导入
    if has_custom {
        row = row.on(ui::Event::Click, &format!("{}{}", BG_LIGHTBOX_PREFIX, code));
    } else {
        row = row.on(ui::Event::Click, &format!("{}{}", BG_PICK_PREFIX, code));
    }

    row
}

fn build_card() -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg(CARD_BG)
        .radius(18)
        .padding(14)
        .gap(10)
}

fn build_section_title(text: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::P, Some(text))
        .size(13)
        .text_color(MUTED)
        .margin_left(4)
        .margin_top(6)
}

fn build_spacer() -> ui::Element {
    ui::Element::new(ui::ElementType::Div, None)
        .flex_grow(1.0)
        .child(ui::Element::new(ui::ElementType::Div, None))
}

fn build_small_button(label: &str, event_id: &str, bg: &str, text_color: &str) -> ui::Element {
    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(16)
        .padding_top(8)
        .padding_bottom(8)
        .padding_left(14)
        .padding_right(14)
        .bg(bg)
        .text_color(text_color)
        .flex()
        .align_center()
        .justify_center()
        .child(ui::Element::new(ui::ElementType::Span, Some(label)).size(13))
}

fn build_icon_button(icon_svg: String, event_id: &str, bg: &str, color: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(18)
        .height(18)
        .text_color(color);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(999)
        .width(32)
        .height(32)
        .bg(bg)
        .flex()
        .align_center()
        .justify_center()
        .child(icon)
}