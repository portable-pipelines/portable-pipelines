//! Parent-side reverse SHV: STEM/VAR in a live CALLPIPE talk to the helper.

use pipe_helper::protocol::{
    decode_shv_reply, encode_shv_request, SHV, SHV_DROP, SHV_GET, SHV_LIST, SHV_REPLY, SHV_SET,
};
use pipe_helper::{with_varpool_child, ChildIo};
use pipe_stages::{VarPoolOp, VarPoolResult};

/// If a helper CALLPIPE is in flight, send the op to that helper's Regina pool.
pub fn try_parent_op(op: &VarPoolOp) -> Option<Result<VarPoolResult, String>> {
    with_varpool_child(|child| rpc(child, op))
}

fn rpc(child: &mut ChildIo, op: &VarPoolOp) -> Result<VarPoolResult, String> {
    let (code, name, value) = match op {
        VarPoolOp::Get(name) => (SHV_GET, name.as_str(), ""),
        VarPoolOp::Set(name, value) => (SHV_SET, name.as_str(), value.as_str()),
        VarPoolOp::List => (SHV_LIST, "", ""),
        VarPoolOp::Drop(name) => (SHV_DROP, name.as_str(), ""),
    };
    child
        .write_frame(SHV, &encode_shv_request(code, name, value))
        .map_err(|e| format!("variable pool request: {e}"))?;
    let frame = child
        .read_frame()
        .map_err(|e| format!("variable pool reply: {e}"))?;
    if frame.typ != SHV_REPLY {
        return Err(format!(
            "unexpected variable pool reply 0x{:02x}",
            frame.typ
        ));
    }
    let reply = decode_shv_reply(&frame.payload, code).map_err(|e| e.to_string())?;
    if !reply.ok {
        return Err(if reply.message.is_empty() {
            "variable pool request failed".into()
        } else {
            reply.message
        });
    }
    match op {
        VarPoolOp::Get(_) => Ok(VarPoolResult::Value(reply.value)),
        VarPoolOp::List => Ok(VarPoolResult::List(reply.list)),
        VarPoolOp::Set(_, _) | VarPoolOp::Drop(_) => Ok(VarPoolResult::Success),
    }
}
