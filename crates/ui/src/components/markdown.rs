use gpui::prelude::*;
use gpui::*;
use crate::Theme;

/// Formats a raw LaTeX mathematical expression into a readable Unicode math string.
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

    // Fractions: \frac{a}{b} -> (a)/(b)
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

/// Renders a formatted Markdown + LaTeX response into styled GPUI elements.
pub fn render_markdown(text: &str, theme: &Theme) -> Div {
    let mut container = div().flex().flex_col().gap_2().w_full();

    // Check if message is split into code blocks, math blocks, and normal paragraphs
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // 1. Fenced Code Block (```lang)
        if trimmed.starts_with("```") {
            let lang = trimmed.trim_start_matches('`').trim().to_string();
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                code_lines.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1; // skip closing ```
            }
            let code_content = code_lines.join("\n");

            container = container.child(
                div()
                    .flex()
                    .flex_col()
                    .rounded_lg()
                    .bg(theme.surface_elevated)
                    .border_1()
                    .border_color(theme.border_subtle)
                    .p_3()
                    .my_1()
                    .when(!lang.is_empty(), |p| {
                        p.child(
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
                        )
                    })
                    .child(
                        div()
                            .text_xs()
                            .line_height(px(18.0))
                            .text_color(theme.text_primary)
                            .child(code_content),
                    ),
            );
            continue;
        }

        // 2. LaTeX Display Math Block ($$...$$ or \[...\])
        if trimmed.starts_with("$$") || trimmed.starts_with("\\[") {
            let mut math_lines = Vec::new();
            if (trimmed.starts_with("$$") && trimmed.ends_with("$$") && trimmed.len() > 4)
                || (trimmed.starts_with("\\[") && trimmed.ends_with("\\]") && trimmed.len() > 4)
            {
                math_lines.push(trimmed);
                i += 1;
            } else {
                math_lines.push(line);
                i += 1;
                while i < lines.len() {
                    let l = lines[i].trim();
                    math_lines.push(lines[i]);
                    i += 1;
                    if l.ends_with("$$") || l.ends_with("\\]") {
                        break;
                    }
                }
            }
            let raw_math = math_lines.join(" ");
            let formatted = format_latex(&raw_math);

            container = container.child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px_4()
                    .py_2()
                    .my_1()
                    .rounded_lg()
                    .bg(Rgba { a: 0.15, ..theme.accent_primary })
                    .border_1()
                    .border_color(theme.border_subtle)
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.text_primary)
                            .child(formatted),
                    ),
            );
            continue;
        }

        // 3. Headings (#, ##, ###)
        if trimmed.starts_with("### ") {
            let heading_text = &trimmed[4..];
            container = container.child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .pt_2()
                    .pb_1()
                    .child(render_inline_text(heading_text, theme)),
            );
            i += 1;
            continue;
        } else if trimmed.starts_with("## ") {
            let heading_text = &trimmed[3..];
            container = container.child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .pt_2()
                    .pb_1()
                    .child(render_inline_text(heading_text, theme)),
            );
            i += 1;
            continue;
        } else if trimmed.starts_with("# ") {
            let heading_text = &trimmed[2..];
            container = container.child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.text_primary)
                    .pt_2()
                    .pb_1()
                    .child(render_inline_text(heading_text, theme)),
            );
            i += 1;
            continue;
        }

        // 4. Blockquotes (> )
        if trimmed.starts_with("> ") {
            let quote_text = &trimmed[2..];
            container = container.child(
                div()
                    .flex()
                    .gap_2()
                    .pl_3()
                    .border_l_2()
                    .border_color(theme.accent_primary)
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.text_secondary)
                            .child(render_inline_text(quote_text, theme)),
                    ),
            );
            i += 1;
            continue;
        }

        // 5. Unordered list (- , * )
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item_text = &trimmed[2..];
            container = container.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .pl_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.accent_primary)
                            .pt(px(4.0))
                            .child("•"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_sm()
                            .line_height(px(22.0))
                            .text_color(theme.text_primary)
                            .child(render_inline_text(item_text, theme)),
                    ),
            );
            i += 1;
            continue;
        }

        // 6. Ordered list (1. , 2. )
        if let Some(dot_idx) = trimmed.find(". ") {
            if dot_idx > 0 && trimmed[..dot_idx].chars().all(|c| c.is_ascii_digit()) {
                let num_str = &trimmed[..=dot_idx];
                let item_text = &trimmed[dot_idx + 2..];
                container = container.child(
                    div()
                        .flex()
                        .items_start()
                        .gap_2()
                        .pl_2()
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme.text_muted)
                                .pt(px(2.0))
                                .child(num_str.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .text_sm()
                                .line_height(px(22.0))
                                .text_color(theme.text_primary)
                                .child(render_inline_text(item_text, theme)),
                        ),
                );
                i += 1;
                continue;
            }
        }

        // 7. Empty line spacer
        if trimmed.is_empty() {
            container = container.child(div().h(px(4.0)));
            i += 1;
            continue;
        }

        // 8. Regular paragraph (processes inline math, bold, code, photo tags)
        container = container.child(
            div()
                .text_sm()
                .line_height(px(22.0))
                .text_color(theme.text_primary)
                .child(render_inline_text(line, theme)),
        );
        i += 1;
    }

    container
}

/// Renders a single line of text with inline markdown styles (bold, code, photo, LaTeX math).
pub fn render_inline_text(text: &str, theme: &Theme) -> Div {
    let mut row = div().flex().flex_wrap().items_center().gap_1();

    // Check if text has LaTeX math, code, or photo tags
    let mut remaining = text;

    while !remaining.is_empty() {
        // Find earliest delimiter: ` (code), $ (math), ** (bold), [Photo: (photo)
        let backtick = remaining.find('`');
        let dollar = remaining.find('$');
        let bold = remaining.find("**");
        let photo = remaining.find("[Photo:");

        let next_delim = [
            backtick.map(|pos| (pos, "code")),
            dollar.map(|pos| (pos, "math")),
            bold.map(|pos| (pos, "bold")),
            photo.map(|pos| (pos, "photo")),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|&(pos, _)| pos);

        match next_delim {
            Some((pos, "code")) => {
                if pos > 0 {
                    row = row.child(div().child(remaining[..pos].to_string()));
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
                    row = row.child(div().child(remaining.to_string()));
                    break;
                }
            }
            Some((pos, "math")) => {
                if pos > 0 {
                    row = row.child(div().child(remaining[..pos].to_string()));
                }
                let rest = &remaining[pos + 1..];
                if let Some(end) = rest.find('$') {
                    let math_str = &rest[..end];
                    let formatted = format_latex(math_str);
                    row = row.child(
                        div()
                            .px_1()
                            .py(px(1.0))
                            .rounded_sm()
                            .bg(Rgba { a: 0.12, ..theme.accent_primary })
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(formatted),
                    );
                    remaining = &rest[end + 1..];
                } else {
                    row = row.child(div().child(remaining.to_string()));
                    break;
                }
            }
            Some((pos, "bold")) => {
                if pos > 0 {
                    row = row.child(div().child(remaining[..pos].to_string()));
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
                    row = row.child(div().child(remaining.to_string()));
                    break;
                }
            }
            Some((pos, "photo")) => {
                if pos > 0 {
                    row = row.child(div().child(remaining[..pos].to_string()));
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
                    row = row.child(div().child(remaining.to_string()));
                    break;
                }
            }
            _ => {
                row = row.child(div().child(remaining.to_string()));
                break;
            }
        }
    }

    row
}
