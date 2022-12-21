#[cfg(not(miri))]
#[test]
fn compile() {
	let t = trybuild::TestCases::new();
	t.pass("tests/compile/pass/*.rs");
	t.compile_fail("tests/compile/fail/*.rs");
}
