"""Today's loop as wires: dev_loop's compose (src/bin/worker/dev_loop/mod.rs) written to
weaver-harness-loop-contract section 8, a test fixture of act 1. The worker's dev_loop
suite (dev_loop/tests/wires.rs) runs compose and these wires against one recording seat
and compares the asks in order, their arguments, and the delta's bytes."""
import json
import re

SYSTEM_PROMPT = "You are a careful assistant. Answer from what you know, say plainly when you do not know, and keep answers as short as the question allows."
REENTRY_OPENING = "The working context was reset to stay within its limit. Recent conversation, restored from the session's record:"
UNREACHABLE = "\n(The record could not be reached: continue from the request alone.)"
CONTINUITY = "Continuity: this session held %d earlier %s with %d completed %s before this one. The conversation content of those runs is not in your context."
RECALL_TURNS = 4
QUOTE_BOUND = 600
U64_MAX = 18446744073709551615
DEPTH_LIMIT = 128
WHITE_SPACE = "[\t\n\x0b\x0c\r \x85\xa0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+"
SURROGATE = "[\ud800-\udfff]"


def message(role, text):
    return {"role": role, "content": [{"type": "text", "text": text}]}


def pressured(resident, capacity):
    # Rust's saturating_mul: Python's integers do not saturate.
    return capacity > 0 and min(resident * 5, U64_MAX) >= min(capacity * 4, U64_MAX)


def no_constant(written):
    raise ValueError("serde_json has no " + written)


def finite_float(written):
    value = float(written)
    if value == float("inf") or value == float("-inf"):
        raise ValueError("a number past a double's range")
    return value


def finite_int(written):
    value = int(written)
    if value > U64_MAX or value < -(U64_MAX // 2) - 1:
        # serde_json reads it as a double, and refuses one past a double's range.
        float(value)
    return value


def nesting(value):
    deepest = 0
    stack = [(value, 1)]
    while stack:
        item, level = stack.pop()
        if isinstance(item, list):
            children = item
        elif isinstance(item, tuple):
            # An object as read: every member, a name given twice among them.
            children = [pair[0] for pair in item] + [pair[1] for pair in item]
        else:
            continue
        deepest = max(deepest, level)
        for child in children:
            stack.append((child, level + 1))
    return deepest


def has_surrogate(value):
    stack = [value]
    while stack:
        item = stack.pop()
        if isinstance(item, str):
            if re.search(SURROGATE, item):
                return True
        elif isinstance(item, list):
            stack.extend(item)
        elif isinstance(item, tuple):
            for pair in item:
                stack.extend(pair)
    return False


def every_member(pairs):
    """An object as read, each member kept: JSON makes no tuple, so a tuple is an object."""
    return tuple(pairs)


def as_value(item):
    """The parse with each object a dict, the last member of a name standing, as serde_json's
    Value keeps it. Called after the depth check, so the nesting is under 128."""
    if isinstance(item, list):
        return [as_value(child) for child in item]
    if isinstance(item, tuple):
        value = {}
        for pair in item:
            value[pair[0]] = as_value(pair[1])
        return value
    return item


def parse_content(content):
    """content parsed as serde_json::from_str::<Value> parses it at 1.0.151, or None."""
    try:
        value = json.loads(content, parse_constant=no_constant, parse_float=finite_float,
                           parse_int=finite_int, object_pairs_hook=every_member)
    except (ValueError, RecursionError, OverflowError):
        return None
    # serde_json counts down from 128 and refuses at 0: 127 levels parse, 128 do not. It
    # refuses as it reads, so a member a later one of the same name replaces is judged too.
    if nesting(value) >= DEPTH_LIMIT or has_surrogate(value):
        return None
    return as_value(value)


def quoted_texts(content, budget):
    blocks = parse_content(content)
    if not isinstance(blocks, list):
        return [], budget
    lines = []
    for block in blocks:
        if not isinstance(block, dict) or not isinstance(block.get("text"), str):
            continue
        if budget == 0:
            continue
        kept = ""
        # Rust's split_whitespace splits on White_Space, which str.split() does not keep to.
        for piece in re.split(WHITE_SPACE, block["text"]):
            if not piece:
                continue
            size = len(piece.encode("utf-8"))
            if size > budget:
                budget = 0
                kept = kept + " ..."
                break
            if kept:
                kept = kept + " "
            kept = kept + piece
            budget = budget - size
        lines.append(kept)
    return lines, budget


def reentry_text(seated, recalled):
    text = "" if seated else SYSTEM_PROMPT + "\n\n"
    text = text + REENTRY_OPENING
    if recalled is None:
        return text + UNREACHABLE
    budget = QUOTE_BOUND
    for event in recalled:
        if event["kind"] == "message.user":
            speaker = "user"
        elif event["kind"] == "message.assistant":
            speaker = "assistant"
        else:
            continue
        contents = [value for key, value in event["pairs"] if key == "content"]
        if not contents:
            continue
        lines, budget = quoted_texts(contents[0], budget)
        for line in lines:
            text = text + "\n" + speaker + ": " + line
    return text


def continuity_line(shape):
    prior = len(shape["runs"]) - 1
    if prior <= 0:
        return None
    turns = 0
    for run in shape["runs"][:prior]:
        counts = [count for kind, count in run["kinds"] if kind == "turn.closed"]
        # A u64 sum, wrapping as the deployed release build does.
        turns = (turns + (counts[0] if counts else 0)) % (U64_MAX + 1)
    runs_word = "run" if prior == 1 else "runs"
    turns_word = "turn" if turns == 1 else "turns"
    return CONTINUITY % (prior, runs_word, turns, turns_word)


def contribution(first_turn, seated, continuity, reentry, text):
    delta = []
    if first_turn and not seated:
        if continuity is None:
            delta.append(message("system", SYSTEM_PROMPT))
        else:
            delta.append(message("system", SYSTEM_PROMPT + "\n\n" + continuity))
    if reentry is not None:
        delta.append(message("system", reentry))
    delta.append(message("user", text))
    return delta


def on_turn(board, text):
    seated = board.seated()
    if board.turn_context() == "independent":
        if board.flush(0) is None:
            board.refuse()
            return
        board.turn(contribution(True, seated, None, None, text))
        return
    first_turn = board.first_turn()
    reentry = None
    if not first_turn:
        fullness = board.fullness()
        if fullness is not None and pressured(fullness["resident"], fullness["capacity"]):
            # The flush is driven for its effect, and its answer gates nothing.
            board.flush(0)
            reentry = reentry_text(seated, board.recall(RECALL_TURNS))
    continuity = None
    if first_turn and not seated:
        shape = board.shape()
        if shape is not None:
            continuity = continuity_line(shape)
    board.turn(contribution(first_turn, seated, continuity, reentry, text))
