"""Fail-closed parser for src/main.rs::print_result, not guest console text."""
import re

HEADER = "========== Execution Result =========="
FOOTER = "====================================="
FAILURE = "simulator-or-runner-failure"


def classify(stdout, stderr, returncode, *, timed_out=False, max_cycles=1000000):
    """Require one terminal host block; preserve diagnostics on every rejection.

    Guest UART output precedes the block and is not searched for result fields.
    The text CLI has no authenticated framing: guest output imitating a complete
    block is ambiguous and is rejected rather than choosing the last block.
    """
    def reject(reason, result=None):
        return {"classification": FAILURE, "reason": reason, "execution_result": result}

    if timed_out:
        return reject("host-timeout")
    if returncode is None or returncode < 0:
        return reject("missing-returncode-or-process-signal")
    if stderr:
        return reject("simulator-stderr")
    lines = stdout.splitlines()
    if lines.count(HEADER) != 1 or lines.count(FOOTER) != 1:
        return reject("missing-or-duplicate-result-block")
    start, end = lines.index(HEADER), lines.index(FOOTER)
    if end <= start or any(line.strip() for line in lines[end + 1:]):
        return reject("unterminated-or-nonterminal-result-block")
    # Fixed ordering/spacing is the actual public renderer, including optional
    # Error and Signature fields. Full matching rejects missing/duplicate/extra
    # fields as well as contradictory statuses, not just the first occurrence.
    match = re.fullmatch(
        r"Exit Code:  (0|[1-9][0-9]*)\n"
        r"Cycles:     (0|[1-9][0-9]*)\n"
        r"Final PC:   (0x[0-9a-f]{16})\n"
        r"Status:     (SUCCESS|FAILED|TIMEOUT)"
        r"(?:\nError:      ([^\n]+))?"
        r"(?:\nSignature:  (0x[0-9a-f]{16}) \((0|[1-9][0-9]*) bytes\))?",
        "\n".join(lines[start + 1:end]),
    )
    if match is None:
        return reject("malformed-or-incomplete-result-fields")
    code, cycles, pc, status, error, signature, size = match.groups()
    # Bound decimal length before int conversion, including Python's digit cap.
    if len(code) > 10 or len(cycles) > 20 or (size and len(size) > 20):
        return reject("numeric-field-out-of-range")
    code, cycles = int(code), int(cycles)
    result = {"exit_code": code, "cycles": cycles, "final_pc": pc,
              "status": status, "error": error}
    if code > 0xffffffff or cycles > 0xffffffffffffffff or cycles > max_cycles:
        return reject("numeric-field-out-of-range", result)
    if size and int(size) > 0xffffffffffffffff:
        return reject("numeric-field-out-of-range", result)
    # Rust exits with exit_code as i32; POSIX exposes its low eight bits.
    # A nonzero guest exit whose process status wraps to zero is not a pass.
    if returncode != (code & 0xff):
        return reject("exit-code-returncode-mismatch", result)
    if status == "TIMEOUT":
        return reject("cycle-timeout", result)
    if (status == "SUCCESS") != (code == 0):
        return reject("status-exit-code-mismatch", result)
    if error is not None:
        return reject("simulator-error", result)
    if status == "FAILED" and returncode == 0:
        return reject("guest-failure-with-zero-process-status", result)
    return {"classification": "guest-pass" if code == 0 else "guest-fail",
            "reason": None, "execution_result": result}
