use crate::ast::NodeData;
use crate::parse::Code;
use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use once_cell::sync::Lazy;

static INSTRUCTION_END: Lazy<AhoCorasick> =
    Lazy::new(|| AhoCorasickBuilder::new().build(["?>"]).unwrap());

pub fn parse_instruction(code: &mut Code) -> NodeData {
    debug_assert!(code.as_slice().starts_with(b"<?"));
    code.shift(2);
    let data = if code.opts.treat_brace_as_opaque || code.opts.treat_chevron_percent_as_opaque {
        code.slice_and_shift_template_aware_until(b"?>").to_vec()
    } else {
        let len = INSTRUCTION_END
            .find(code.as_slice())
            .map_or(code.rem(), |m| m.start());
        code.copy_and_shift(len)
    };
    let ended = code.shift_if_next_seq_case_insensitive(b"?>");
    NodeData::Instruction { code: data, ended }
}
