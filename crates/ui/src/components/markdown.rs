use gpui::prelude::*;
use gpui::*;
use crate::Theme;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, Alignment};

/// Formats a raw LaTeX mathematical expression into a readable Unicode math string,
/// or cleans raw math notation.
pub fn format_latex(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    // Remove enclosing delimiters if present
    if s.starts_with("$$") && s.ends_with("$$") && s.len() >= 4 {
        s = s[2..s.len() - 2].trim().to_string();
    } else if s.starts_with('$') && s.ends_with('$') && s.len() >= 2 {
        s = s[1..s.len() - 1].trim().to_string();
    } else if s.starts_with("\\[") && s.ends_with("\\]") && s.len() >= 4 {
        s = s[2..s.len() - 2].trim().to_string();
    } else if s.starts_with("\\(") && s.ends_with("\\)") && s.len() >= 4 {
        s = s[2..s.len() - 2].trim().to_string();
    }

    // Common Greek Letters
    s = s.replace("\\alpha", "α")
        .replace("\\beta", "β")
        .replace("\\gamma", "γ")
        .replace("\\delta", "δ")
        .replace("\\epsilon", "ε")
        .replace("\\zeta", "ζ")
        .replace("\\eta", "η")
        .replace("\\theta", "θ")
        .replace("\\iota", "ι")
        .replace("\\kappa", "κ")
        .replace("\\lambda", "λ")
        .replace("\\mu", "μ")
        .replace("\\nu", "ν")
        .replace("\\xi", "ξ")
        .replace("\\pi", "π")
        .replace("\\rho", "ρ")
        .replace("\\sigma", "σ")
        .replace("\\tau", "τ")
        .replace("\\upsilon", "υ")
        .replace("\\phi", "φ")
        .replace("\\chi", "χ")
        .replace("\\psi", "ψ")
        .replace("\\omega", "ω")
        .replace("\\Gamma", "Γ")
        .replace("\\Delta", "Δ")
        .replace("\\Theta", "Θ")
        .replace("\\Lambda", "Λ")
        .replace("\\Xi", "Ξ")
        .replace("\\Pi", "Π")
        .replace("\\Sigma", "Σ")
        .replace("\\Phi", "Φ")
        .replace("\\Psi", "Ψ")
        .replace("\\Omega", "Ω");

    // Math Operators & Symbols
    s = s.replace("\\times", "×")
        .replace("\\cdot", "·")
        .replace("\\div", "÷")
        .replace("\\pm", "±")
        .replace("\\mp", "∓")
        .replace("\\le", "≤")
        .replace("\\leq", "≤")
        .replace("\\ge", "≥")
        .replace("\\geq", "≥")
        .replace("\\neq", "≠")
        .replace("\\approx", "≈")
        .replace("\\equiv", "≡")
        .replace("\\sim", "∼")
        .replace("\\propto", "∝")
        .replace("\\infty", "∞")
        .replace("\\in", "∈")
        .replace("\\notin", "∉")
        .replace("\\subset", "⊂")
        .replace("\\subseteq", "⊆")
        .replace("\\cup", "∪")
        .replace("\\cap", "∩")
        .replace("\\forall", "∀")
        .replace("\\exists", "∃")
        .replace("\\nabla", "∇")
        .replace("\\partial", "∂")
        .replace("\\int", "∫")
        .replace("\\sum", "∑")
        .replace("\\prod", "∏")
        .replace("\\lim", "lim")
        .replace("\\to", "→")
        .replace("\\rightarrow", "→")
        .replace("\\leftarrow", "←")
        .replace("\\Rightarrow", "⇒")
        .replace("\\Leftarrow", "⇐")
        .replace("\\leftrightarrow", "↔")
        .replace("\\Leftrightarrow", "⇔")
        .replace("\\dots", "…")
        .replace("\\ldots", "…")
        .replace("\\quad", "  ")
        .replace("\\qquad", "    ");

    // Fractions: \frac{a}{b} -> (a / b)
    while let Some(pos) = s.find("\\frac{") {
        let after = &s[pos + 6..];
        if let Some(mid) = after.find("}{") {
            let num = &after[..mid];
            let rest = &after[mid + 2..];
            if let Some(end) = rest.find('}') {
                let den = &rest[..end];
                let replacement = format!("({} / {})", num, den);
                let full_len = 6 + mid + 2 + end + 1;
                s.replace_range(pos..pos + full_len, &replacement);
                continue;
            }
        }
        break;
    }

    // Square roots: \sqrt{x} -> √(x)
    while let Some(pos) = s.find("\\sqrt{") {
        let after = &s[pos + 6..];
        if let Some(end) = after.find('}') {
            let inside = &after[..end];
            let replacement = format!("√({})", inside);
            let full_len = 6 + end + 1;
            s.replace_range(pos..pos + full_len, &replacement);
            continue;
        }
        break;
    }

    // Common superscripts
    s = s.replace("^0", "⁰")
        .replace("^1", "¹")
        .replace("^2", "²")
        .replace("^3", "³")
        .replace("^4", "⁴")
        .replace("^5", "⁵")
        .replace("^6", "⁶")
        .replace("^7", "⁷")
        .replace("^8", "⁸")
        .replace("^9", "⁹")
        .replace("^n", "ⁿ")
        .replace("^x", "ˣ")
        .replace("^i", "ⁱ")
        .replace("^+", "⁺")
        .replace("^-", "⁻")
        .replace("^{2}", "²")
        .replace("^{3}", "³")
        .replace("^{n}", "ⁿ")
        .replace("^{x}", "ˣ")
        .replace("^{0}", "⁰")
        .replace("^{1}", "¹")
        .replace("^{4}", "⁴")
        .replace("^{5}", "⁵")
        .replace("^{6}", "⁶")
        .replace("^{7}", "⁷")
        .replace("^{8}", "⁸")
        .replace("^{9}", "⁹");

    // Common subscripts
    s = s.replace("_0", "₀")
        .replace("_1", "₁")
        .replace("_2", "₂")
        .replace("_3", "₃")
        .replace("_4", "₄")
        .replace("_5", "₅")
        .replace("_6", "₆")
        .replace("_7", "₇")
        .replace("_8", "₈")
        .replace("_9", "₉")
        .replace("_i", "ᵢ")
        .replace("_j", "ⱼ")
        .replace("_k", "ₖ")
        .replace("_n", "ₙ")
        .replace("_x", "ₓ")
        .replace("_{0}", "₀")
        .replace("_{1}", "₁")
        .replace("_{2}", "₂")
        .replace("_{3}", "₃")
        .replace("_{i}", "ᵢ")
        .replace("_{n}", "ₙ");

    // Clean up braces and formatting commands
    s = s.replace("\\left(", "(")
        .replace("\\right)", ")")
        .replace("\\left[", "[")
        .replace("\\right]", "]")
        .replace("\\left\\{", "{")
        .replace("\\right\\}", "}")
        .replace("\\{", "{")
        .replace("\\}", "}")
        .replace("\\text{", "")
        .replace("\\mathbf{", "")
        .replace("\\mathit{", "")
        .replace("\\mathrm{", "");

    s
}

/// Represents parsed Math node for rich rendering
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MathNode {
    Text(String),
    Fraction { num: String, den: String },
    Sqrt { content: String, root: Option<String> },
    Matrix { rows: Vec<Vec<String>> },
    Superscript { base: String, exp: String },
    Subscript { base: String, sub: String },
}

/// Parse a LaTeX math string into a sequence of MathNodes for visual layout rendering
pub fn parse_math_nodes(raw: &str) -> Vec<MathNode> {
    let cleaned = format_latex(raw);
    let mut s = raw.trim();

    // Unwrap outer delimiters
    if (s.starts_with("$$") && s.ends_with("$$") && s.len() >= 4)
        || (s.starts_with("\\[") && s.ends_with("\\]") && s.len() >= 4)
    {
        s = s[2..s.len() - 2].trim();
    } else if (s.starts_with('$') && s.ends_with('$') && s.len() >= 2)
        || (s.starts_with("\\(") && s.ends_with("\\)") && s.len() >= 4)
    {
        s = &s[1..s.len() - 1].trim();
    }

    // Check for Matrix environment (\begin{matrix} or \begin{pmatrix} or \begin{bmatrix})
    if let Some(begin_idx) = s.find("\\begin{") {
        if s.find("matrix}").is_some() {
            let env_start = s[begin_idx..].find('}').map(|i| begin_idx + i + 1).unwrap_or(begin_idx);
            let env_end = s.rfind("\\end{").unwrap_or(s.len());
            if env_start < env_end {
                let body = &s[env_start..env_end];
                let row_strs: Vec<&str> = body.split("\\\\").collect();
                let mut rows = Vec::new();
                for r in row_strs {
                    let cols: Vec<String> = r.split('&').map(|c| format_latex(c.trim())).collect();
                    if !cols.is_empty() && (cols.len() > 1 || !cols[0].is_empty()) {
                        rows.push(cols);
                    }
                }
                if !rows.is_empty() {
                    return vec![MathNode::Matrix { rows }];
                }
            }
        }
    }

    // Check for standalone fraction: \frac{num}{den}
    if s.starts_with("\\frac{") {
        if let Some(mid) = s[6..].find("}{") {
            let num_raw = &s[6..6 + mid];
            let rest = &s[6 + mid + 2..];
            if let Some(end) = rest.find('}') {
                let den_raw = &rest[..end];
                if 6 + mid + 2 + end + 1 == s.len() {
                    return vec![MathNode::Fraction {
                        num: format_latex(num_raw),
                        den: format_latex(den_raw),
                    }];
                }
            }
        }
    }

    // Default fallback to text with Unicode math replacements
    vec![MathNode::Text(cleaned)]
}

/// Renders a MathNode sequence into styled GPUI layout elements
pub fn render_math_node(node: &MathNode, theme: &Theme) -> Div {
    match node {
        MathNode::Text(t) => div()
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_primary)
            .child(t.clone()),
        MathNode::Fraction { num, den } => div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(num.clone()),
            )
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(theme.text_primary)
                    .my(px(1.0)),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(den.clone()),
            ),
        MathNode::Sqrt { content, root: _ } => div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .child("√"),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(theme.text_primary)
                    .px_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(content.clone()),
            ),
        MathNode::Matrix { rows } => {
            let mut matrix_col = div()
                .flex()
                .flex_col()
                .gap_1()
                .px_2()
                .py_1()
                .border_l_2()
                .border_r_2()
                .border_color(theme.accent_primary)
                .rounded_sm();

            for row in rows {
                let mut row_div = div().flex().items_center().gap_3();
                for col in row {
                    row_div = row_div.child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(col.clone()),
                    );
                }
                matrix_col = matrix_col.child(row_div);
            }
            matrix_col
        }
        MathNode::Superscript { base, exp } => div()
            .flex()
            .items_start()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(base.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.accent_primary)
                    .child(exp.clone()),
            ),
        MathNode::Subscript { base, sub } => div()
            .flex()
            .items_end()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_primary)
                    .child(base.clone()),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.accent_primary)
                    .child(sub.clone()),
            ),
    }
}

/// Renders display LaTeX block
pub fn render_display_math(raw_math: &str, theme: &Theme) -> Div {
    let nodes = parse_math_nodes(raw_math);

    let mut container = div()
        .flex()
        .items_center()
        .justify_center()
        .flex_wrap()
        .gap_2()
        .px_4()
        .py_2()
        .my_1()
        .w_full()
        .max_w_full()
        .overflow_x_hidden()
        .rounded_lg()
        .bg(Rgba { a: 0.12, ..theme.accent_primary })
        .border_1()
        .border_color(theme.border_subtle);

    for node in &nodes {
        container = container.child(render_math_node(node, theme));
    }

    container
}

/// Helper struct to hold streaming / inline math segments
struct InlineMathSegment {
    is_math: bool,
    content: String,
}

/// Preprocesses inline text safely separating `$math$` math blocks from normal text
fn extract_inline_math(text: &str) -> Vec<InlineMathSegment> {
    let mut segments = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if let Some(start) = remaining.find('$') {
            if start > 0 {
                segments.push(InlineMathSegment {
                    is_math: false,
                    content: remaining[..start].to_string(),
                });
            }
            let rest = &remaining[start + 1..];
            if let Some(end) = rest.find('$') {
                let math_str = &rest[..end];
                if !math_str.is_empty() {
                    segments.push(InlineMathSegment {
                        is_math: true,
                        content: math_str.to_string(),
                    });
                } else {
                    segments.push(InlineMathSegment {
                        is_math: false,
                        content: "$$".to_string(),
                    });
                }
                remaining = &rest[end + 1..];
            } else {
                // Incomplete streaming inline math token
                segments.push(InlineMathSegment {
                    is_math: true,
                    content: rest.to_string(),
                });
                break;
            }
        } else {
            segments.push(InlineMathSegment {
                is_math: false,
                content: remaining.to_string(),
            });
            break;
        }
    }

    segments
}

/// AST-based Markdown & LaTeX Renderer
pub fn render_markdown(text: &str, theme: &Theme) -> Div {
    let mut container = div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .max_w_full()
        .overflow_hidden();

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    // Pre-extract display math blocks ($$...$$ or \[...\]) before passing text to pulldown-cmark parser
    let lines: Vec<&str> = text.lines().collect();
    let mut line_idx = 0;

    while line_idx < lines.len() {
        let line = lines[line_idx];
        let trimmed = line.trim();

        // Check for Display Math block
        if trimmed.starts_with("$$") || trimmed.starts_with("\\[") {
            let mut math_lines = Vec::new();
            if (trimmed.starts_with("$$") && trimmed.ends_with("$$") && trimmed.len() > 4)
                || (trimmed.starts_with("\\[") && trimmed.ends_with("\\]") && trimmed.len() > 4)
            {
                math_lines.push(trimmed);
                line_idx += 1;
            } else {
                math_lines.push(line);
                line_idx += 1;
                while line_idx < lines.len() {
                    let l = lines[line_idx].trim();
                    math_lines.push(lines[line_idx]);
                    line_idx += 1;
                    if l.ends_with("$$") || l.ends_with("\\]") {
                        break;
                    }
                }
            }
            let raw_math = math_lines.join("\n");
            container = container.child(render_display_math(&raw_math, theme));
            continue;
        }

        // Collect normal block lines until next display math
        let mut block_lines = Vec::new();
        while line_idx < lines.len() {
            let l = lines[line_idx].trim();
            if l.starts_with("$$") || l.starts_with("\\[") {
                break;
            }
            block_lines.push(lines[line_idx]);
            line_idx += 1;
        }

        if block_lines.is_empty() {
            continue;
        }

        let block_str = block_lines.join("\n");
        let parser = Parser::new_ext(&block_str, opts);

        // State machine variables for processing pulldown-cmark events
        let mut current_code_lang: Option<String> = None;
        let mut in_code_block = false;
        let mut code_block_text = String::new();

        let mut in_table = false;
        let mut table_alignments: Vec<Alignment> = Vec::new();
        let mut current_table_headers: Vec<String> = Vec::new();
        let mut current_table_rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut current_cell_text = String::new();
        let mut in_table_head = false;

        let mut list_stack: Vec<Option<u64>> = Vec::new(); // Some(start_num) for ordered, None for unordered
        let mut in_blockquote = false;
        let mut blockquote_text = String::new();

        let mut paragraph_text = String::new();
        let mut current_heading_level: Option<u32> = None;
        let mut heading_text = String::new();

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_block_text.clear();
                        current_code_lang = match kind {
                            pulldown_cmark::CodeBlockKind::Fenced(lang) => {
                                let l = lang.to_string();
                                if l.is_empty() { None } else { Some(l) }
                            }
                            pulldown_cmark::CodeBlockKind::Indented => None,
                        };
                    }
                    Tag::Table(aligns) => {
                        in_table = true;
                        table_alignments = aligns;
                        current_table_headers.clear();
                        current_table_rows.clear();
                    }
                    Tag::TableHead => {
                        in_table_head = true;
                    }
                    Tag::TableRow => {
                        current_row.clear();
                    }
                    Tag::TableCell => {
                        current_cell_text.clear();
                    }
                    Tag::List(first_num) => {
                        list_stack.push(first_num);
                    }
                    Tag::Item => {
                        paragraph_text.clear();
                    }
                    Tag::BlockQuote(_) => {
                        in_blockquote = true;
                        blockquote_text.clear();
                    }
                    Tag::Heading { level, .. } => {
                        current_heading_level = Some(level as u32);
                        heading_text.clear();
                    }
                    Tag::Paragraph => {
                        paragraph_text.clear();
                    }
                    Tag::Strong => {
                        if current_heading_level.is_some() {
                            heading_text.push_str("**");
                        } else {
                            paragraph_text.push_str("**");
                        }
                    }
                    Tag::Emphasis => {
                        if current_heading_level.is_some() {
                            heading_text.push('*');
                        } else {
                            paragraph_text.push('*');
                        }
                    }
                    Tag::Strikethrough => {
                        if current_heading_level.is_some() {
                            heading_text.push_str("~~");
                        } else {
                            paragraph_text.push_str("~~");
                        }
                    }
                    _ => {}
                },
                Event::End(tag) => match tag {
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                        let lang = current_code_lang.take().unwrap_or_default();
                        let code_content = code_block_text.clone();

                        let mut code_box = div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .max_w_full()
                            .overflow_x_hidden()
                            .rounded_lg()
                            .bg(theme.surface_elevated)
                            .border_1()
                            .border_color(theme.border_subtle)
                            .p_3()
                            .my_1();

                        if !lang.is_empty() {
                            code_box = code_box.child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .pb_1()
                                    .mb_1()
                                    .border_b_1()
                                    .border_color(theme.border_subtle)
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(theme.text_muted)
                                            .child(lang.to_uppercase()),
                                    ),
                            );
                        }

                        code_box = code_box.child(
                            div()
                                .text_xs()
                                .line_height(px(18.0))
                                .text_color(theme.text_primary)
                                .child(code_content),
                        );

                        container = container.child(code_box);
                    }
                    TagEnd::TableCell => {
                        if in_table_head {
                            current_table_headers.push(current_cell_text.clone());
                        } else {
                            current_row.push(current_cell_text.clone());
                        }
                    }
                    TagEnd::TableRow => {
                        if !in_table_head && !current_row.is_empty() {
                            current_table_rows.push(current_row.clone());
                        }
                    }
                    TagEnd::TableHead => {
                        in_table_head = false;
                    }
                    TagEnd::Table => {
                        in_table = false;
                        // Render responsive horizontal scrolling table
                        let headers = current_table_headers.clone();
                        let rows = current_table_rows.clone();
                        let aligns = table_alignments.clone();

                        let mut table_element = div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .max_w_full()
                            .overflow_x_hidden()
                            .my_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme.border_subtle)
                            .bg(theme.surface_elevated);

                        // Header row
                        if !headers.is_empty() {
                            let mut header_row = div()
                                .flex()
                                .items_center()
                                .bg(theme.surface_input)
                                .border_b_1()
                                .border_color(theme.border_subtle);

                            for (idx, header_cell) in headers.iter().enumerate() {
                                let align = aligns.get(idx).cloned().unwrap_or(Alignment::None);
                                header_row = header_row.child(
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .border_r_1()
                                        .border_color(theme.border_subtle)
                                        .text_xs()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.text_primary)
                                        .when(align == Alignment::Center, |p| p.text_center())
                                        .when(align == Alignment::Right, |p| p.text_right())
                                        .child(header_cell.clone()),
                                );
                            }
                            table_element = table_element.child(header_row);
                        }

                        // Table data rows
                        for (r_idx, row) in rows.iter().enumerate() {
                            let mut row_div = div()
                                .flex()
                                .items_center()
                                .when(r_idx % 2 == 1, |p| p.bg(theme.surface_input))
                                .when(r_idx < rows.len() - 1, |p| p.border_b_1().border_color(theme.border_subtle));

                            for (c_idx, cell) in row.iter().enumerate() {
                                let align = aligns.get(c_idx).cloned().unwrap_or(Alignment::None);
                                row_div = row_div.child(
                                    div()
                                        .flex_1()
                                        .px_3()
                                        .py_2()
                                        .border_r_1()
                                        .border_color(theme.border_subtle)
                                        .text_xs()
                                        .text_color(theme.text_secondary)
                                        .when(align == Alignment::Center, |p| p.text_center())
                                        .when(align == Alignment::Right, |p| p.text_right())
                                        .child(cell.clone()),
                                );
                            }
                            table_element = table_element.child(row_div);
                        }

                        container = container.child(table_element);
                    }
                    TagEnd::List(_) => {
                        list_stack.pop();
                    }
                    TagEnd::Item => {
                        let depth = list_stack.len().saturating_sub(1);
                        let is_ordered = list_stack.last().and_then(|o| *o).is_some();
                        let indent_px = (depth * 16) + 8;

                        let bullet = if is_ordered {
                            format!("{}.", list_stack.last().and_then(|o| *o).unwrap_or(1))
                        } else {
                            "•".to_string()
                        };

                        container = container.child(
                            div()
                                .flex()
                                .items_start()
                                .gap_2()
                                .pl(px(indent_px as f32))
                                .child(
                                    div()
                                        .text_xs()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(theme.accent_primary)
                                        .pt(px(2.0))
                                        .child(bullet),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .line_height(px(22.0))
                                        .text_color(theme.text_primary)
                                        .child(render_inline_text(&paragraph_text, theme)),
                                ),
                        );
                        paragraph_text.clear();
                    }
                    TagEnd::BlockQuote(_) => {
                        in_blockquote = false;
                        container = container.child(
                            div()
                                .flex()
                                .gap_2()
                                .pl_3()
                                .my_1()
                                .border_l_2()
                                .border_color(theme.accent_primary)
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(theme.text_secondary)
                                        .child(render_inline_text(&blockquote_text, theme)),
                                ),
                        );
                        blockquote_text.clear();
                    }
                    TagEnd::Heading(_) => {
                        let level = current_heading_level.take().unwrap_or(1);
                        let heading_el = match level {
                            1 => div().text_lg().font_weight(FontWeight::BOLD).text_color(theme.text_primary).pt_2().pb_1(),
                            2 => div().text_base().font_weight(FontWeight::BOLD).text_color(theme.text_primary).pt_2().pb_1(),
                            _ => div().text_sm().font_weight(FontWeight::BOLD).text_color(theme.text_primary).pt_1().pb_1(),
                        };
                        container = container.child(heading_el.child(render_inline_text(&heading_text, theme)));
                        heading_text.clear();
                    }
                    TagEnd::Paragraph => {
                        if !paragraph_text.is_empty() && list_stack.is_empty() {
                            container = container.child(
                                div()
                                    .text_sm()
                                    .line_height(px(22.0))
                                    .text_color(theme.text_primary)
                                    .child(render_inline_text(&paragraph_text, theme)),
                            );
                            paragraph_text.clear();
                        }
                    }
                    TagEnd::Strong => {
                        if current_heading_level.is_some() {
                            heading_text.push_str("**");
                        } else {
                            paragraph_text.push_str("**");
                        }
                    }
                    TagEnd::Emphasis => {
                        if current_heading_level.is_some() {
                            heading_text.push('*');
                        } else {
                            paragraph_text.push('*');
                        }
                    }
                    TagEnd::Strikethrough => {
                        if current_heading_level.is_some() {
                            heading_text.push_str("~~");
                        } else {
                            paragraph_text.push_str("~~");
                        }
                    }
                    _ => {}
                },
                Event::Text(t) => {
                    let text_str = t.as_ref();
                    if in_code_block {
                        code_block_text.push_str(text_str);
                    } else if in_table {
                        current_cell_text.push_str(text_str);
                    } else if in_blockquote {
                        blockquote_text.push_str(text_str);
                    } else if current_heading_level.is_some() {
                        heading_text.push_str(text_str);
                    } else {
                        paragraph_text.push_str(text_str);
                    }
                }
                Event::Code(c) => {
                    let code_str = format!("`{}`", c.as_ref());
                    if current_heading_level.is_some() {
                        heading_text.push_str(&code_str);
                    } else {
                        paragraph_text.push_str(&code_str);
                    }
                }
                Event::Rule => {
                    container = container.child(
                        div()
                            .w_full()
                            .h(px(1.0))
                            .bg(theme.border_subtle)
                            .my_2(),
                    );
                }
                Event::SoftBreak | Event::HardBreak => {
                    if in_code_block {
                        code_block_text.push('\n');
                    } else if current_heading_level.is_some() {
                        heading_text.push(' ');
                    } else {
                        paragraph_text.push(' ');
                    }
                }
                _ => {}
            }
        }

        // Flush trailing paragraph if any
        if !paragraph_text.is_empty() && list_stack.is_empty() {
            container = container.child(
                div()
                    .text_sm()
                    .line_height(px(22.0))
                    .text_color(theme.text_primary)
                    .child(render_inline_text(&paragraph_text, theme)),
            );
        }
    }

    container
}

fn append_text_words(mut row: Div, text: &str) -> Div {
    let mut words = text.split(' ').peekable();
    while let Some(word) = words.next() {
        if !word.is_empty() {
            row = row.child(div().child(word.to_string()));
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_latex_greek_and_operators() {
        let raw = "$$\\alpha + \\beta = \\gamma \\times \\infty$$";
        let formatted = format_latex(raw);
        assert_eq!(formatted, "α + β = γ × ∞");
    }

    #[test]
    fn test_format_latex_fractions_and_roots() {
        let raw = "\\frac{a + 1}{b - 1} + \\sqrt{x^2 + y^2}";
        let formatted = format_latex(raw);
        assert_eq!(formatted, "(a + 1 / b - 1) + √(x² + y²)");
    }

    #[test]
    fn test_parse_math_nodes_matrix() {
        let raw = "$$\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}$$";
        let nodes = parse_math_nodes(raw);
        assert_eq!(nodes.len(), 1);
        if let MathNode::Matrix { rows } = &nodes[0] {
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0], vec!["1", "2"]);
            assert_eq!(rows[1], vec!["3", "4"]);
        } else {
            panic!("Expected MathNode::Matrix");
        }
    }

    #[test]
    fn test_extract_inline_math_complete_and_streaming() {
        let text = "Here is $x + 1$ and incomplete streaming $y +";
        let segs = extract_inline_math(text);
        assert_eq!(segs.len(), 3);
        assert!(!segs[0].is_math);
        assert_eq!(segs[0].content, "Here is ");
        assert!(segs[1].is_math);
        assert_eq!(segs[1].content, "x + 1");
        assert!(segs[2].is_math);
        assert_eq!(segs[2].content, "y +");
    }
}

/// Renders a single line of text with inline markdown styles (bold, code, photo, LaTeX math).
pub fn render_inline_text(text: &str, theme: &Theme) -> Div {
    let mut row = div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap_1()
        .w_full()
        .max_w_full()
        .overflow_hidden();

    // Safely extract inline math blocks `$ ... $`
    let math_segments = extract_inline_math(text);

    for seg in math_segments {
        if seg.is_math {
            let nodes = parse_math_nodes(&format!("${}$", seg.content));
            let mut math_div = div()
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .py(px(1.0))
                .rounded_sm()
                .bg(Rgba { a: 0.12, ..theme.accent_primary });

            for node in &nodes {
                math_div = math_div.child(render_math_node(node, theme));
            }
            row = row.child(math_div);
        } else {
            let mut remaining = seg.content.as_str();
            while !remaining.is_empty() {
                let backtick = remaining.find('`');
                let bold = remaining.find("**");
                let photo = remaining.find("[Photo:");

                let next_delim = [
                    backtick.map(|pos| (pos, "code")),
                    bold.map(|pos| (pos, "bold")),
                    photo.map(|pos| (pos, "photo")),
                ]
                .into_iter()
                .flatten()
                .min_by_key(|&(pos, _)| pos);

                match next_delim {
                    Some((pos, "code")) => {
                        if pos > 0 {
                            row = append_text_words(row, &remaining[..pos]);
                        }
                        let rest = &remaining[pos + 1..];
                        if let Some(end) = rest.find('`') {
                            let code_str = &rest[..end];
                            row = row.child(
                                div()
                                    .px_1()
                                    .py(px(1.0))
                                    .rounded_sm()
                                    .bg(theme.surface_elevated)
                                    .border_1()
                                    .border_color(theme.border_subtle)
                                    .text_xs()
                                    .text_color(theme.accent_primary)
                                    .child(code_str.to_string()),
                            );
                            remaining = &rest[end + 1..];
                        } else {
                            row = append_text_words(row, remaining);
                            break;
                        }
                    }
                    Some((pos, "bold")) => {
                        if pos > 0 {
                            row = append_text_words(row, &remaining[..pos]);
                        }
                        let rest = &remaining[pos + 2..];
                        if let Some(end) = rest.find("**") {
                            let bold_str = &rest[..end];
                            row = row.child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(bold_str.to_string()),
                            );
                            remaining = &rest[end + 2..];
                        } else {
                            row = append_text_words(row, remaining);
                            break;
                        }
                    }
                    Some((pos, "photo")) => {
                        if pos > 0 {
                            row = append_text_words(row, &remaining[..pos]);
                        }
                        let rest = &remaining[pos..];
                        if let Some(end) = rest.find(']') {
                            let tag = &rest[..=end];
                            row = row.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .px_2()
                                    .py_0p5()
                                    .rounded_md()
                                    .bg(Rgba { a: 0.2, ..theme.accent_primary })
                                    .border_1()
                                    .border_color(theme.accent_primary)
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(format!("📷 {}", tag.trim_start_matches("[Photo: ").trim_end_matches(']'))),
                            );
                            remaining = &rest[end + 1..];
                        } else {
                            row = append_text_words(row, remaining);
                            break;
                        }
                    }
                    _ => {
                        row = append_text_words(row, remaining);
                        break;
                    }
                }
            }
        }
    }

    row
}
