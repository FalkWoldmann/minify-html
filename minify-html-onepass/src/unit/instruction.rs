use crate::err::ProcessingResult;
use crate::proc::MatchAction::*;
use crate::proc::MatchMode::*;
use crate::proc::Processor;
use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use std::sync::LazyLock;

static INSTRUCTION_END: LazyLock<AhoCorasick> =
    LazyLock::new(|| AhoCorasickBuilder::new().build(["?>"]).unwrap());

#[inline(always)]
pub fn process_instruction(proc: &mut Processor) -> ProcessingResult<()> {
    proc.m(IsSeq(b"<?"), Keep).expect();
    proc.m(ThroughSeq(&INSTRUCTION_END), Keep)
        .require("instruction end")?;
    Ok(())
}
