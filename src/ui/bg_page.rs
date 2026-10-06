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
/// 压暗 / 模糊卡片里图标与名称同排，和 15 号名称搭配用 20 更贴（安卓端也是 20dp）
const SLIDER_ICON_SIZE: u32 = 20;
/// 滑块自身上下额外留的间隔，叠在卡片的 10 间距上
const SLIDER_EXTRA_GAP: u32 = 4;
const ROW_THUMB_SIZE: u32 = 40;
/// 说明 / 导入 / 导出三个条目的最小列宽，和列表一样按宽度自动决定列数
const ACTION_MIN_COLUMN: u32 = 180;
/// 压暗 / 模糊两张卡片的最小列宽，排不下两张时自然叠成一列
const EDIT_CARD_MIN_COLUMN: u32 = 240;
/// 说明 / 导入 / 导出外面那层卡片的内边距
const ACTION_CARD_PADDING: u32 = 8;

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
            "已配置 {}/12 张背景图",
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

    build_card().padding(ACTION_CARD_PADDING).child(
        ui::Element::new(ui::ElementType::Grid, None)
            .width_full()
            .prop(
                "columns",
                // min(…, 100%)：容器比最小列宽还窄时不会被钉在最小宽度上
                &format!(
                    "repeat(auto-fit, minmax(min({}px, 100%), 1fr))",
                    ACTION_MIN_COLUMN
                ),
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

/// 使用说明子页：文案与安卓端 `BackgroundImagePickerActivity` 的帮助面板一致，
/// 只有指路的地方按插件自己的控件改写（发送按钮在状态卡右侧、导入导出是两个入口）。
fn build_guide() -> ui::Element {
    // 内容与安卓端 `BackgroundImagePickerActivity` 的帮助面板逐条一致，
    // 只有指路的部分按插件实际控件改写（发送按钮在状态卡右侧、导入导出是两个入口）。
    // 空标题的行是多段/多行的续行，渲染时不再重复小标题。
    let sections: [(&str, &str); 10] = [
        (
            "选择背景图",
            "点击每种天气类型右侧的 + 按钮，从相册中选择一张图片作为该天气的自定义背景。支持 12 种天气类型，每种可单独设置。",
        ),
        (
            "同步到手表",
            "配置好背景图后，点击「背景图传输」卡片右侧的发送按钮即可将所有背景图同步到手表端。同步过程中请不要操作手表。",
        ),
        (
            "传输说明",
            "背景图同步采用覆盖传输模式，每次同步会将所有已配置的图片重新发送到手表端。如果同步过程中某张图片传输失败，你可以暂时删除其他已成功传输的图片，仅保留失败的那一张，然后再次点击发送单独重传该图片即可，无需全部重新传输。",
        ),
        (
            "",
            "传输完成后，请退出手表端应用再重新进入，才能看到新背景效果。若覆盖的是手表上已有的自定义背景图，因设备图片缓存问题，还需重启手环刷新缓存后才会显示新图。",
        ),
        (
            "导入 / 导出预设包",
            "点击「导入预设包」或「导出预设包」可导入 / 导出 .swbg 格式的预设包，方便备份和分享。",
        ),
        (
            "分享预设包",
            "将当前所有背景配置打包分享给其他人，对方可直接导入使用。",
        ),
        ("图片处理参数", "· 压暗：调整背景图亮度，数值越大越暗。"),
        ("", "· 模糊：对背景图应用高斯模糊效果。"),
        (
            "",
            "同步到手表时会自动压缩画质以加快传输。调节滑块后可实时预览效果。",
        ),
        (
            "清除背景图",
            "若所有天气都未选图，点击同步按钮会弹出清除确认，可将手表端已存储的自定义背景图全部清除，恢复默认背景。",
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
        .child(ui::Element::new(ui::ElementType::P, Some("自定义背景图指南")).size(17));

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
        // 续行（空标题）贴着上一段，不再重复小标题
        let mut row = ui::Element::new(ui::ElementType::Div, None)
            .flex()
            .flex_direction(ui::FlexDirection::Column)
            .width_full()
            .padding_top(if title.is_empty() { 2 } else { 10 })
            .padding_bottom(10)
            .gap(2);
        if !title.is_empty() {
            row = row.child(ui::Element::new(ui::ElementType::P, Some(title)).size(15));
        }
        row = row.child(
            ui::Element::new(ui::ElementType::P, Some(body))
                .size(13)
                .text_color("#888888"),
        );
        list = list.child(row);
    }

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
    // 收图和推图共用这张卡：左侧状态、右侧一颗圆形动作按钮（发送 / 取消）
    let transferring = crate::bg::session::is_transferring();

    let percent = if progress.total_chunks > 0 {
        progress.received.saturating_mul(100) / progress.total_chunks
    } else {
        0
    };
    // 重算进度不展示：分帧跑完很快，卡片高度保持固定即可
    let detail = if transferring {
        format!(
            "{}  {}/{}  {}%",
            progress.message, progress.received, progress.total_chunks, percent
        )
    } else if progress.message.trim().is_empty() {
        "等待传输".to_string()
    } else {
        progress.message.clone()
    };

    let text_col = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .flex_grow(1.0)
        .gap(2)
        .child(ui::Element::new(ui::ElementType::P, Some("背景图传输")).size(15))
        // 固定单行高度：文案在"等待/接收/重算/完成"之间切换时不撑高卡片，
        // 否则下面的"已配置 N/12"会被顶得上下跳
        .child(
            ui::Element::new(ui::ElementType::P, Some(&detail))
                .size(13)
                .height(STATUS_LINE_HEIGHT)
                .text_color(if transferring { ACCENT } else { MUTED }),
        );

    let action = if transferring {
        build_round_action(icons::x_svg(), BG_CANCEL_EVENT, "#FF453A26", DANGER)
    } else {
        build_round_action(icons::send_tab_svg(), BG_SEND_EVENT, "#0090FF26", ACCENT)
    };

    build_card()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .child(text_col)
        .child(action)
}

/// 状态卡右侧的圆形动作按钮：空闲是「发送到手表」，传输中是「取消传输」。
/// 比列表里的图标按钮大一号（40 / 20），当作这一页的主操作。
fn build_round_action(icon_svg: String, event_id: &str, bg: &str, color: &str) -> ui::Element {
    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(20)
        .height(20)
        .text_color(color);

    ui::Element::new(ui::ElementType::Button, None)
        .without_default_styles()
        .on(ui::Event::Click, event_id)
        .radius(999)
        .width(40)
        .height(40)
        .bg(bg)
        .flex()
        .align_center()
        .justify_center()
        .child(icon)
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
                "repeat(auto-fit, minmax(min({}px, 100%), 1fr))",
                EDIT_CARD_MIN_COLUMN
            ),
        )
        .prop("gap", &format!("{}px", crate::ui::state::ROW_COLUMN_GAP))
        .child(darken)
        .child(blur)
}

/// 单项设置卡片：图标与名称同一行，滑块在下面占满整行（不露具体数值，与安卓端一致）。
/// 图标 20 与 15 号名称同排更贴，文字样式和内边距沿用设置列表的卡片。
fn build_slider_row(icon_svg: String, label: &str, value: u32, event_id: &str) -> ui::Element {
    let max_prop = SLIDER_MAX.to_string();
    let value_prop = value.to_string();

    let icon = ui::Element::new(ui::ElementType::Svg, Some(&icon_svg))
        .width(SLIDER_ICON_SIZE)
        .height(SLIDER_ICON_SIZE)
        .text_color("#FFFFFF");

    let title_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Row)
        .align_center()
        .width_full()
        .gap(10)
        .child(icon)
        .child(ui::Element::new(ui::ElementType::P, Some(label)).size(15));

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

    // 滑块自身上下左右各多留一点：上下靠外边距，左右靠内边距（宽度撑满的容器加左右
    // 外边距会溢出，用内边距不会）
    let slider_row = ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .margin_top(SLIDER_EXTRA_GAP)
        .margin_bottom(SLIDER_EXTRA_GAP)
        .padding_left(SLIDER_EXTRA_GAP)
        .padding_right(SLIDER_EXTRA_GAP)
        .child(slider);

    ui::Element::new(ui::ElementType::Div, None)
        .flex()
        .flex_direction(ui::FlexDirection::Column)
        .width_full()
        .bg(CARD_BG)
        .radius(18)
        .padding_left(12)
        .padding_right(12)
        .padding_top(10)
        .padding_bottom(10)
        .gap(10)
        .child(title_row)
        .child(slider_row)
}

/// 12 个天气编号的列表：每项自成一张卡片，按宽度自适应列数
fn build_code_list_card(state: &UiState) -> ui::Element {
    // 两种宽度共用同一套卡片条目，只有列数不同：窄屏一列、宽屏多列
    let mut grid = ui::Element::new(ui::ElementType::Grid, None)
        .width_full()
        .prop(
            "columns",
            &format!(
                "repeat(auto-fit, minmax(min({}px, 100%), 1fr))",
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
