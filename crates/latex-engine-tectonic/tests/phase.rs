use latex_engine_tectonic::detect_phase;
use latex_model::BuildPhase;

#[test]
fn plain_compile_phase_is_done() {
    let out = "note: Running TeX ...\nnote: Running xdvipdfmx ...\n";
    assert_eq!(detect_phase(out, "", true), BuildPhase::Done);
}

#[test]
fn bibtex_phase_is_observed() {
    let out = "note: Running TeX ...\nnote: Running BibTeX on main.aux ...\n";
    assert_eq!(detect_phase(out, "", true), BuildPhase::Done);
    // On failure the phase reflects the last note we saw.
    assert_eq!(detect_phase(out, "", false), BuildPhase::Bibliography);
}

#[test]
fn rerun_after_bibtex() {
    let out = "note: Running TeX ...\nnote: Running BibTeX on main.aux ...\nnote: Rerunning TeX because bibtex was run ...\n";
    assert_eq!(detect_phase(out, "", false), BuildPhase::Rerun);
}

#[test]
fn rendering_phase() {
    let out = "note: Running xdvipdfmx ...\n";
    assert_eq!(detect_phase(out, "", false), BuildPhase::Rendering);
}

#[test]
fn empty_output_is_started() {
    assert_eq!(detect_phase("", "", false), BuildPhase::Started);
}
