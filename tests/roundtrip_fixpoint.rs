//! Round-trip fixpoint tests.
//!
//! For LaTeX math `L`, convert `T1 = latex_to_typst(L)`, then `L2 =
//! typst_to_latex(T1)` in math-only mode, then `T2 = latex_to_typst(L2)`. Once
//! the first round has normalised the input, the second round must change
//! nothing: `T1 == T2`. A difference means one direction loses or invents
//! information, without a human reading either output. A second check runs
//! `check_latex` on `L2`: an error there means T → L wrote invalid LaTeX, even
//! when `T2` still matches.
//!
//! The corpus test pins which entries of `corpus_merged.json` are unstable
//! today and why, so it fails both on a new instability and when a known one
//! is fixed. The generator test compares tokens, since Typst ignores
//! differences in spacing.

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use tylax::diagnostics::{check_latex, DiagnosticLevel};
use tylax::{latex_to_typst, typst_to_latex_with_options, T2LOptions};
use typst_syntax::{SyntaxKind, SyntaxNode};

struct RoundTrip {
    t1: String,
    l2: String,
    t2: String,
}

fn round_trip(latex: &str) -> RoundTrip {
    let t1 = latex_to_typst(latex).trim().to_string();
    let l2 = typst_to_latex_with_options(&t1, &T2LOptions::math_only())
        .trim()
        .to_string();
    let t2 = latex_to_typst(&l2).trim().to_string();
    RoundTrip { t1, l2, t2 }
}

/// The error messages `check_latex` reports for `latex`: unbalanced braces or
/// environments, and parser syntax errors. Its warnings and notes are about
/// support for a command or package, not validity, so they are left out.
fn latex_errors(latex: &str) -> Vec<String> {
    check_latex(latex)
        .diagnostics
        .into_iter()
        .filter(|diagnostic| diagnostic.level == DiagnosticLevel::Error)
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// The leaf tokens of `typst` parsed as math, without whitespace. Two outputs
/// with the same tokens differ only in spacing, which Typst ignores between
/// tokens (`f (x)= x` and `f (x) = x`), while `ab` and `a b` stay distinct.
fn tokens(typst: &str) -> Vec<String> {
    fn walk(node: &SyntaxNode, out: &mut Vec<String>) {
        if node.children().len() == 0 && node.kind() != SyntaxKind::Space {
            out.push(node.text().to_string());
        }
        for child in node.children() {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(&typst_syntax::parse_math(typst), &mut out);
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Stable,
    /// `T1` and `T2` have the same tokens but different whitespace.
    Spacing,
    Unstable,
    /// `check_latex` reports an error in `L2`, whatever `T2` is.
    InvalidLatex,
}

fn outcome(round: &RoundTrip) -> Outcome {
    if !latex_errors(&round.l2).is_empty() {
        Outcome::InvalidLatex
    } else if round.t1 == round.t2 {
        Outcome::Stable
    } else if tokens(&round.t1) == tokens(&round.t2) {
        Outcome::Spacing
    } else {
        Outcome::Unstable
    }
}

/// Why a corpus entry is not a fixpoint, with a minimal input for each.
#[derive(Clone, Copy, Debug)]
enum Cause {
    /// L → T copies source whitespace: `F(x)=x` → `F (x)= x`, then `F (x) = x`.
    Spacing,
    /// L → T writes two scripts of one kind in a row, which Typst nests:
    /// `{x^{a}}^{b}` → `x^(a)^(b)` → `x^{\left(a\right)^b}`.
    StackedScripts,
    /// L → T escapes a delimiter that pairs only across a TeX group, and the
    /// group is gone on the way back: `\left\{ x { \right\} }` → `{ x \}`
    /// → `\{x \}` → `{x }`.
    CrossGroupDelimiter,
}

/// Corpus entries that are not a fixpoint today, by cause. Where an entry has
/// more than one cause, it is listed under the first one the diff shows.
const KNOWN_UNSTABLE: &[(Cause, &[&str])] = &[
    (
        Cause::Spacing,
        &[
            "m0", "m1", "m2", "m3", "m7", "m9", "m11", "m33", "m34", "m35", "m36", "m51", "m68",
            "m69", "m79", "m82", "m83", "m107", "m116", "m117", "m119", "m121", "m122", "m125",
            "m127", "m128", "m129", "m130", "m131", "m132", "m136", "m165", "m177", "m189", "m190",
            "m191", "m192", "m193", "m217", "m231", "m232", "m234", "m235", "m239", "m241", "m243",
            "m246",
        ],
    ),
    // m305 is a LaTeX double subscript, `_{a}_{b}`.
    (Cause::StackedScripts, &["m305"]),
    (Cause::CrossGroupDelimiter, &["m310"]),
];

fn expected_outcome(id: &str) -> (Outcome, Option<Cause>) {
    for (cause, ids) in KNOWN_UNSTABLE {
        if ids.contains(&id) {
            let outcome = match cause {
                Cause::Spacing => Outcome::Spacing,
                _ => Outcome::Unstable,
            };
            return (outcome, Some(*cause));
        }
    }
    (Outcome::Stable, None)
}

#[test]
fn merged_corpus_round_trip_is_a_fixpoint() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("corpus_merged.json")).expect("valid merged corpus JSON");
    let ids: Vec<&str> = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect();
    for (_, known) in KNOWN_UNSTABLE {
        for id in *known {
            assert!(
                ids.contains(id),
                "KNOWN_UNSTABLE lists `{id}`, which is not in the corpus"
            );
        }
    }

    let mut mismatches = Vec::new();
    for case in &cases {
        let id = case["id"].as_str().unwrap();
        let latex = case["latex"].as_str().unwrap();
        let round = round_trip(latex);
        let got = outcome(&round);
        let (expected, cause) = expected_outcome(id);
        if got != expected {
            mismatches.push(format!(
                "  {id}: expected {expected:?} (cause {cause:?}), got {got:?}\n    L1: {latex}\n    T1: {}\n    L2: {}\n    T2: {}",
                round.t1, round.l2, round.t2
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "round-trip outcomes changed; update KNOWN_UNSTABLE:\n{}",
        mismatches.join("\n")
    );
}

// Grammar of valid LaTeX math for the generator tests.

const GREEK: &[&str] = &[
    r"\alpha",
    r"\beta",
    r"\gamma",
    r"\delta",
    r"\epsilon",
    r"\varepsilon",
    r"\zeta",
    r"\eta",
    r"\theta",
    r"\vartheta",
    r"\iota",
    r"\kappa",
    r"\lambda",
    r"\mu",
    r"\nu",
    r"\xi",
    r"\pi",
    r"\rho",
    r"\sigma",
    r"\tau",
    r"\upsilon",
    r"\phi",
    r"\varphi",
    r"\chi",
    r"\psi",
    r"\omega",
    r"\Gamma",
    r"\Delta",
    r"\Theta",
    r"\Lambda",
    r"\Xi",
    r"\Pi",
    r"\Sigma",
    r"\Phi",
    r"\Psi",
    r"\Omega",
];
const SYMBOLS: &[&str] = &[
    r"\infty",
    r"\partial",
    r"\nabla",
    r"\emptyset",
    r"\forall",
    r"\exists",
    r"\neg",
    r"\ell",
    r"\hbar",
    r"\angle",
    r"\triangle",
    r"\cdots",
    r"\ldots",
    r"\prime",
];
const BINARY: &[&str] = &[
    "+",
    "-",
    "/",
    r"\pm",
    r"\mp",
    r"\times",
    r"\div",
    r"\cdot",
    r"\cup",
    r"\cap",
    r"\setminus",
    r"\circ",
    r"\oplus",
    r"\otimes",
    r"\wedge",
    r"\vee",
];
const RELATIONS: &[&str] = &[
    "=",
    "<",
    ">",
    ",",
    r"\leq",
    r"\geq",
    r"\neq",
    r"\approx",
    r"\equiv",
    r"\sim",
    r"\simeq",
    r"\cong",
    r"\propto",
    r"\in",
    r"\notin",
    r"\subset",
    r"\subseteq",
    r"\supset",
    r"\to",
    r"\rightarrow",
    r"\leftarrow",
    r"\Rightarrow",
    r"\Leftrightarrow",
    r"\mapsto",
    r"\perp",
    r"\parallel",
    r"\mid",
];
const FUNCTIONS: &[&str] = &[
    r"\sin", r"\cos", r"\tan", r"\log", r"\ln", r"\exp", r"\max", r"\min", r"\det",
];
const LARGE_OPERATORS: &[&str] = &[r"\sum", r"\prod", r"\int", r"\oint", r"\bigcup", r"\bigcap"];
const FONTS: &[&str] = &[
    r"\mathbf",
    r"\mathrm",
    r"\mathit",
    r"\mathsf",
    r"\mathtt",
    r"\boldsymbol",
];
/// Fonts that only take a capital letter.
const LETTER_FONTS: &[&str] = &[r"\mathcal", r"\mathbb", r"\mathfrak"];
const ACCENTS: &[&str] = &[
    r"\hat",
    r"\bar",
    r"\vec",
    r"\dot",
    r"\ddot",
    r"\tilde",
    r"\overline",
    r"\underline",
    r"\widehat",
    r"\widetilde",
];
const DELIMITERS: &[(&str, &str)] = &[
    ("(", ")"),
    ("[", "]"),
    (r"\{", r"\}"),
    ("|", "|"),
    (r"\|", r"\|"),
    (r"\langle", r"\rangle"),
    (r"\lfloor", r"\rfloor"),
    (r"\lceil", r"\rceil"),
    (".", "."),
];
const MATRICES: &[&str] = &["matrix", "pmatrix", "bmatrix", "vmatrix"];

/// A single symbol: a valid script base on its own.
fn atom() -> BoxedStrategy<String> {
    prop_oneof![
        3 => "[a-zA-Z]",
        2 => "[0-9]{1,3}(\\.[0-9]{1,2})?",
        2 => select(GREEK).prop_map(String::from),
        1 => select(SYMBOLS).prop_map(String::from),
        1 => (select(LETTER_FONTS), "[A-Z]").prop_map(|(font, c)| format!("{font}{{{c}}}")),
    ]
    .boxed()
}

/// Whether a braced group of `latex` may be a script base. L → T drops the
/// braces, so a script or fraction inside joins the outer script:
/// `{x^{a}}^{b}` → `x^(a)^(b)` and `{\frac{a}{b}}_{c}` → `a/b_(c)`. Typst has no
/// invisible grouping to keep them apart, so this cause is won't-fix, and the
/// generator leaves it out so that the tests find others.
fn braced_base_allowed(latex: &str) -> bool {
    !latex.contains(['^', '_']) && !latex.contains(r"\frac")
}

/// A LaTeX math expression. Script bases are atoms or braced groups, so no
/// output has a double script, and `\text` holds only plain words.
fn latex_math() -> BoxedStrategy<String> {
    let leaf = prop_oneof![
        5 => atom(),
        1 => (select(FUNCTIONS), atom()).prop_map(|(f, a)| format!("{f} {a}")),
        1 => "[a-z]'{1,2}",
        1 => "[a-z]{1,6}( [a-z]{1,6})?".prop_map(|t| format!(r"\text{{{t}}}")),
    ];
    leaf.prop_recursive(4, 32, 4, |inner| {
        let base = prop_oneof![
            atom(),
            (atom(), inner.clone()).prop_map(|(a, e)| {
                if braced_base_allowed(&e) {
                    format!("{{{e}}}")
                } else {
                    a
                }
            }),
        ];
        let cell = inner.clone();
        let matrix =
            (select(MATRICES), 1..=3usize, 1..=3usize).prop_flat_map(move |(env, rows, cols)| {
                (Just(env), vec(vec(cell.clone(), cols), rows))
            });
        prop_oneof![
            vec(inner.clone(), 2..4).prop_map(|terms| terms.join(" ")),
            (inner.clone(), select(BINARY), inner.clone())
                .prop_map(|(a, op, b)| format!("{a} {op} {b}")),
            (inner.clone(), select(RELATIONS), inner.clone())
                .prop_map(|(a, op, b)| format!("{a} {op} {b}")),
            inner.clone().prop_map(|a| format!("-{a}")),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| format!(r"\frac{{{a}}}{{{b}}}")),
            (inner.clone(), inner.clone()).prop_map(|(a, b)| format!(r"\binom{{{a}}}{{{b}}}")),
            inner.clone().prop_map(|a| format!(r"\sqrt{{{a}}}")),
            ("[2-9]", inner.clone()).prop_map(|(n, a)| format!(r"\sqrt[{n}]{{{a}}}")),
            (base.clone(), inner.clone()).prop_map(|(b, sup)| format!("{b}^{{{sup}}}")),
            (base.clone(), inner.clone()).prop_map(|(b, sub)| format!("{b}_{{{sub}}}")),
            (base, inner.clone(), inner.clone())
                .prop_map(|(b, sub, sup)| format!("{b}_{{{sub}}}^{{{sup}}}")),
            (
                select(LARGE_OPERATORS),
                inner.clone(),
                inner.clone(),
                inner.clone()
            )
                .prop_map(|(op, lo, hi, body)| format!("{op}_{{{lo}}}^{{{hi}}} {body}")),
            (select(DELIMITERS), inner.clone())
                .prop_map(|((l, r), a)| format!(r"\left{l} {a} \right{r}")),
            (select(FONTS), inner.clone()).prop_map(|(font, a)| format!("{font}{{{a}}}")),
            (select(ACCENTS), inner.clone()).prop_map(|(accent, a)| format!("{accent}{{{a}}}")),
            matrix.prop_map(|(env, rows)| {
                let rows: Vec<String> = rows.iter().map(|row| row.join(" & ")).collect();
                format!(r"\begin{{{env}}} {} \end{{{env}}}", rows.join(r" \\ "))
            }),
            vec((inner.clone(), inner), 1..4).prop_map(|rows| {
                let rows: Vec<String> = rows.iter().map(|(a, b)| format!("{a} & {b}")).collect();
                format!(r"\begin{{cases}} {} \end{{cases}}", rows.join(r" \\ "))
            }),
        ]
    })
    .boxed()
}

/// The number of generated cases per test: `PROPTEST_CASES`, else 1,000.
fn generated_cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(1000)
}

proptest! {
    // A failure prints its input; its fix adds a test to integration_tests.rs
    // rather than a proptest regressions file.
    #![proptest_config(ProptestConfig {
        cases: generated_cases(),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Ignores whitespace, so that spacing does not hide the other causes.
    #[test]
    fn generated_math_round_trip_is_a_fixpoint_up_to_spacing(latex in latex_math()) {
        let round = round_trip(&latex);
        prop_assert_eq!(latex_errors(&round.l2), Vec::<String>::new(), "T1: {}\nL2: {}", round.t1, round.l2);
        prop_assert_eq!(tokens(&round.t1), tokens(&round.t2), "T1: {}\nL2: {}\nT2: {}", round.t1, round.l2, round.t2);
    }
}
