//! 背景图页签：传输状态、图片处理滑块、12 个天气编号的背景图管理。
//!
//! 背景图列表只有一套行式条目（缩略图 + 两行文字 + 右侧动作按钮），
//! 按渲染区宽度自适应列数：窄屏一列，宽屏多列，条目本身完全一致。
//!
//! 交互对齐安卓 `BackgroundImageItem`，并按端上交互做了调整：
//!
//! - 没有图 → 点整张卡片/整行 → 拉起系统图片选择器导入
//! - 已有图 → 右上角 ✕ 删除并恢复默认（二次确认），删完再点该格导入新的
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
/// 说明 / 导入 / 导出三个条目的最小列宽，和列表一样按宽度自动决定列数
const ACTION_MIN_COLUMN: u32 = 180;
/// 压暗 / 模糊两张卡片的最小列宽，排不下两张时自然叠成一列
const EDIT_CARD_MIN_COLUMN: u32 = 240;

/// 传输状态那行的固定高度（单行 13px 文字）
const STATUS_LINE_HEIGHT: u32 = 18;

const CARD_BG: &str = "#1E1E1F";
const CARD_INNER_BG: &str = "#2A2A2A";
const MUTED: &str = "#8E8E93";
const FAINT: &str = "#48484A";
const ACCENT: &str = "#0090FF";
const DANGER: &str = "#FF453A";

pub fn build_bg_tab(state: &UiState) -> ui::Element {
    let root = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8);

    if state.bg_guide {
        return build_guide();
    }

    root.child(build_transfer_card())
        // 与设置页一样：小标题前面那张卡片多留 10，标题与上方卡片之间就是 8 + 10 = 18
        .child(build_preset_card(state).margin_bottom(10))
        .child(super::build::build_section_title("图片处理"))
        .child(build_edit_card(state).margin_bottom(10))
        .child(super::build::build_section_title(&format!(
            "已配置 {}/12 张",
            state.bg_codes.len()
        )))
        .child(build_code_list_card(state))
}

/// 说明 / 导入 / 导出：独立成一个动作容器
///
/// 和顶部那张传输状态卡刻意区分：状态卡是纯信息底色的两行文字，这里是带图标的
/// 可点条目，用淡蓝底 + 强调色图标，一眼能看出是操作而不是状态。
/// 排列方式和下面的背景图列表一致：同一套条目，列数随宽度自适应。
fn build_preset_card(state: &UiState) -> ui::Element {
    let has_images = !state.bg_codes.is_empty();

    let guide = action_entry(icons::info_svg(), "说明", BG_GUIDE_EVENT, ACCENT);
    let import = action_entry(
        icons::download_simple_svg(),
        "导入预设包",
        BG_IMPORT_EVENT,
        ACCENT,
    );
    let export = action_entry(
        icons::upload_simple_svg(),
        "导出预设包",
        BG_EXPORT_EVENT,
        if has_images { ACCENT } else { "#636366" },
    );

    build_card().padding(8).child(
        ui::Element::new(ui::ElementType::Grid, None)
            .width_full()
            .prop(
                "columns",
                // auto-fit：条目不足一整行时折叠空轨道，让现有条目平摊整宽
                &format!("repeat(auto-fit, minmax({}px, 1fr))", ACTION_MIN_COLUMN),
            )
            .prop("gap", &format!("{}px", crate::ui::state::ROW_COLUMN_GAP))
            .child(guide)
            .child(import)
            .child(export),
    )
}

/// 动作条目：图标在左、文字在右，占满一格
fn action_entry(icon_svg: String, label: &str, event_id: &str, color: &str) -> ui::Element {
    let enabled = color == ACCENT;

    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(18)
        .height(18)
        .text_color(color);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(12)
        .bg(if enabled { "#0090FF1F" } else { "#FFFFFF0A" })
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
        .child(
            ui::Element::new(ui::ElementType::Span, Some(label))
                .size(14)
                .text_color(color),
        )
}

/// 使用说明。写的是插件端真实行为，不照搬安卓端那套（端上选图 + 与快应用两套独立存储）。
fn build_guide() -> ui::Element {
    let sections: [(&str, &str); 10] = [
        (
            "两种图片来源",
            "① 端上选图：点没有背景图的卡片（或移动端整行）打开系统文件选择器，支持 PNG / JPG / WEBP，单张上限 12 MB。② 手机端推图：手机「自定义背景图」页点同步，直接覆盖式传到手环。两条路各存各的，互不影响。",
        ),
        (
            "已配置的样子",
            "每条只有右侧按钮可点：没图时是 ＋，点它挑一张导入；已有图时是 ✕，删除并恢复默认（二次确认），删完这一条回到未配置，再点 ＋ 导入新的。条目样式两端一致，窄屏排一列、宽屏排多列。",
        ),
        (
            "发送到手表",
            "顶部「背景图传输」卡片里那颗「发送到手表」会把当前已配置的图按压暗 / 模糊重新出一遍，再逐张覆盖式传给手环，与安卓端「覆盖传输模式」一致。传输中同一个位置变成「取消传输」；手环上点取消也会立刻中止整轮。一张图都没配置时点发送会先二次确认，确认后清除手环上已存的自定义背景图（本机库不动）。",
        ),
        (
            "删除即恢复默认",
            "✕ 会连同原始图片一起删除（删除前二次确认，不可撤销），删完这一格回到未配置状态。注意：删除只影响本插件，手机端和手环快应用各自的副本不受影响。",
        ),
        (
            "压暗与模糊",
            "两个滑块范围都是 0-100，且是全局参数——改一次会按新参数重算所有已配置的背景图，右上状态卡会显示重算进度。改完记得点一次「发送到手表」，手环那份才会跟着更新。",
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
            "发送后的生效时机",
            "发送完成后本机立刻可见；手环快应用那侧的壁纸需要退出快应用再重新进入才刷新，若覆盖的是已有的自定义背景图，可能还要重启手环清图片缓存。",
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

    // 顶部返回栏与「位置设置」子页保持一致：40 的圆形返回键 + 17 号标题
    let back_btn = ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, BG_CLOSE_GUIDE_EVENT)
        .width(40)
        .height(40)
        .radius(999)
        .bg(CARD_INNER_BG)
        .flex()
        .align_center()
        .justify_center()
        .child(
            ui::Element::new(ui::ElementType::Svg, Some(&icons::back_arrow_svg()))
                .width(20)
                .height(20),
        );

    let top_bar = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .gap(8)
        .margin_bottom(8)
        .child(back_btn)
        .child(ui::Element::new(ui::ElementType::P, Some("使用说明")).size(17));

    // 条目列表沿用位置历史 / 搜索结果那套容器与行距
    let mut list = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg(CARD_BG)
        .radius(18)
        .padding_left(12)
        .padding_right(12)
        .padding_top(4)
        .padding_bottom(4);

    for (title, body) in sections {
        list = list.child(
            ui::Element::new(ui::ElementType::Div, None)
                .flex()
                .flex_direction(ui::FlexDirection::Column)
                .width_full()
                .padding_top(10)
                .padding_bottom(10)
                .gap(2)
                .child(ui::Element::new(ui::ElementType::P, Some(title)).size(15))
                .child(
                    ui::Element::new(ui::ElementType::P, Some(body))
                        .size(13)
                        .text_color("#888888"),
                ),
        );
    }

    // 完整文档入口也做成同样的一行，不再单独一颗按钮
    list = list.child(
        ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Row)
            .align_center()
            .justify_center()
            .width_full()
            .padding_top(10)
            .padding_bottom(10)
            .on(ui::Event::Click, OPEN_HELP_DOC_EVENT)
            .child(
                ui::Element::new(ui::ElementType::P, Some("打开完整文档"))
                    .size(15)
                    .text_color(ACCENT),
            ),
    );

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .gap(8)
        .child(top_bar)
        .child(list)
}

/// 传输状态：文案对齐手环端 `image-service.js` 的 message
fn build_transfer_card() -> ui::Element {
    let progress = crate::bg::session::progress();
    // 收图和推图共用这张卡：只要有传输在进行就显示进度与「取消传输」
    let receiving = crate::bg::session::is_transferring();

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
    } else {
        // 空闲时同一个位置就是「发送到手表」：对齐安卓背景图页右下角那颗发送按钮
        row = row.child(build_small_button(
            "发送到手表",
            BG_SEND_EVENT,
            "#0090FF26",
            ACCENT,
        ));
    }
    row
}

/// 图片处理：压暗 + 模糊，两项各自一张卡片，列数随宽度自适应
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

    ui::Element::new(ui::ElementType::Grid, None)
        .width_full()
        .prop(
            "columns",
            &format!(
                "repeat(auto-fit, minmax({}px, 1fr))",
                EDIT_CARD_MIN_COLUMN
            ),
        )
        .prop("gap", &format!("{}px", crate::ui::state::ROW_COLUMN_GAP))
        .child(darken)
        .child(blur)
}

/// 单项设置卡片：图标在左，右侧上下排布「名称 + 数值」和滑块。
/// 图标尺寸、文字样式与内边距都对齐设置列表的卡片。
fn build_slider_row(icon_svg: String, label: &str, value: u32, event_id: &str) -> ui::Element {
    let value_text = value.to_string();
    let max_prop = SLIDER_MAX.to_string();
    let value_prop = value.to_string();

    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(22)
        .height(22)
        .text_color("#FFFFFF");

    let icon_wrap = ui::Element::new(ui::ElementType::Div, None)
        .width(22)
        .height(22)
        .flex()
        .align_center()
        .justify_center()
        .child(icon);

    let title_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(15))
        .child(build_spacer())
        .child(
            ui::Element::new(ui::ElementType::P, Some(&value_text))
                .size(13)
                .text_color("#BBBBBB"),
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

    let text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .flex_grow(1.0)
        .gap(10)
        .child(title_row)
        .child(slider);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg(CARD_BG)
        .radius(18)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .gap(10)
        .child(icon_wrap)
        .child(text_col)
}

/// 12 个天气编号的列表：每项自成一张卡片，按宽度自适应列数
fn build_code_list_card(state: &UiState) -> ui::Element {
    // 两种宽度共用同一套卡片条目，只有列数不同：窄屏一列、宽屏多列
    let mut grid = ui::Element::new(ui::ElementType::Grid, None)
        .width_full()
        .prop(
            "columns",
            &format!(
                "repeat(auto-fit, minmax({}px, 1fr))",
                crate::ui::state::ROW_MIN_COLUMN
            ),
        )
        .prop("gap", &format!("{}px", crate::ui::state::ROW_COLUMN_GAP));
    for (code, label) in crate::bg::protocol::WEATHER_BG_CODES.iter() {
        grid = grid.child(build_code_row(code, label, state));
    }

    grid
}

/// 列表项卡片：对齐安卓 `BackgroundImageItem`——缩略图 + 两行文字 + 右侧一颗图标，
/// 每项一张卡，尺寸与设置列表的卡片一致
///
/// 只有右侧按钮可点：没图是 ＋ 选图，有图是 ✕ 删除。整行不绑事件，
/// 否则按钮的点击会冒泡到行上，同一个选图事件被派发两次。
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

    // 两行文字：天气名 + 副标题（有图是文件名，没图提示走默认背景）
    let mut text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .flex_grow(1.0)
        .gap(2)
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(15));
    let subtitle = if has_custom {
        crate::bg::store::label_of(code)
    } else {
        "使用默认背景".to_string()
    };
    if !subtitle.trim().is_empty() {
        text_col = text_col.child(
            ui::Element::new(ui::ElementType::P, Some(&subtitle))
                .size(12)
                .text_color(MUTED),
        );
    }

    let row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .bg(CARD_BG)
        .radius(18)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .gap(10)
        .child(thumb)
        .child(text_col);

    // 右侧动作按钮：没图是加号（点它选图），有图是叉号（点它删除），与安卓一致
    row.child(if has_custom {
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
    })
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

/// 圆形图标按钮的外观，不绑事件，作为可点区域里的视觉提示
fn build_icon_badge(icon_svg: String, bg: &str, color: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(18)
        .height(18)
        .text_color(color);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .radius(999)
        .width(32)
        .height(32)
        .bg(bg)
        .flex()
        .align_center()
        .justify_center()
        .child(icon)
}

fn build_icon_button(icon_svg: String, event_id: &str, bg: &str, color: &str) -> ui::Element {
    build_icon_badge(icon_svg, bg, color).on(ui::Event::Click, event_id)
}
