use crate::ast::NodeData;
use crate::parse::Code;
use memchr::memchr;

pub fn parse_bang(code: &mut Code) -> NodeData {
    debug_assert!(code.as_slice().starts_with(b"<!"));
    code.shift(2);
    let data = if code.opts.treat_brace_as_opaque || code.opts.treat_chevron_percent_as_opaque {
        code.slice_and_shift_template_aware_until(b">").to_vec()
    } else {
        let len = memchr(b'>', code.as_slice()).unwrap_or(code.rem());
        code.copy_and_shift(len)
    };
    let ended = code.shift_if_next(b'>');
    NodeData::Bang { code: data, ended }
}
