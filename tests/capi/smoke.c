/*
 * Plain-C compile-and-link smoke for the ruscv-sim C ABI (dev-plan D8/D11).
 *
 * The script `scripts/test_capi_smoke.sh` builds the static library with the
 * `capi` feature, compiles this file with the host C compiler and runs it.  It
 * covers the U5 acceptance surface: lifecycle, profile selection, commit-log
 * output, grant time fields and earlier-start rejection, wrong-thread
 * rejection, re-entry rejection, panic poisoning, a transport status that is
 * not a passing result, and an unknown completion followed by reset.
 *
 * The guest is a hand-built ELF fixture (the same shape as
 * `tests/common/public_elf.rs`) so the smoke needs only a host C compiler.
 */

#include "ruscv_sim.h"

#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define TICK_FS 1000000u
#define BASE 0x80000000ull
#define SEGMENT_OFFSET 0x1000u
#define TOHOST_OFFSET 0x1000u
#define TOHOST (BASE + TOHOST_OFFSET)
#define FILE_DATA_SIZE 0x2008u
#define MEMORY_SIZE 0x4000u

#define RAM_SIZE 0x10000u

static int failures;

static void check(int condition, const char *what)
{
    if (condition) {
        printf("PASS  %s\n", what);
    } else {
        printf("FAIL  %s\n", what);
        failures += 1;
    }
    fflush(stdout);
}

/* ------------------------------------------------------------ ELF fixture */

static void put_u16(uint8_t *b, size_t o, uint16_t v)
{
    b[o] = (uint8_t)(v & 0xff);
    b[o + 1] = (uint8_t)(v >> 8);
}

static void put_u32(uint8_t *b, size_t o, uint32_t v)
{
    put_u16(b, o, (uint16_t)(v & 0xffff));
    put_u16(b, o + 2, (uint16_t)(v >> 16));
}

static void put_u64(uint8_t *b, size_t o, uint64_t v)
{
    put_u32(b, o, (uint32_t)(v & 0xffffffffu));
    put_u32(b, o + 4, (uint32_t)(v >> 32));
}

static void put_section(uint8_t *elf, size_t o, uint32_t name, uint32_t type,
                        uint64_t flags, uint64_t address, uint64_t offset,
                        uint64_t size, uint64_t align)
{
    put_u32(elf, o, name);
    put_u32(elf, o + 4, type);
    put_u64(elf, o + 8, flags);
    put_u64(elf, o + 16, address);
    put_u64(elf, o + 24, offset);
    put_u64(elf, o + 32, size);
    put_u64(elf, o + 48, align);
}

/* RV64I encoders, matching `tests/common/public_elf.rs`. */
static uint32_t enc_addi(int rd, int rs1, int imm)
{
    return ((uint32_t)imm & 0xfffu) << 20 | (uint32_t)rs1 << 15 | (uint32_t)rd << 7 | 0x13u;
}

static uint32_t enc_lui(int rd, uint32_t imm20)
{
    return imm20 << 12 | (uint32_t)rd << 7 | 0x37u;
}

static uint32_t enc_slli(int rd, int rs1, int shift)
{
    return (uint32_t)shift << 20 | (uint32_t)rs1 << 15 | 1u << 12 | (uint32_t)rd << 7 | 0x13u;
}

static uint32_t enc_sd(int rs2, int rs1, int imm)
{
    uint32_t uimm = (uint32_t)imm & 0xfffu;
    return (uimm >> 5) << 25 | (uint32_t)rs2 << 20 | (uint32_t)rs1 << 15 | 3u << 12 |
           (uimm & 0x1fu) << 7 | 0x23u;
}

/*
 * Writes one ELF64/RISC-V executable whose guest reports HTIF exit `code`
 * through the declared `.tohost` section.
 *
 * The first parcel is two compressed `c.nop`s, so the image only executes under
 * an IALIGN=16 profile: every profile without compression traps on it.
 */
static int build_guest(const char *path, uint32_t code)
{
    uint32_t instructions[] = {
        0x00010001u,                     /* c.nop; c.nop (profile-sensitive) */
        enc_lui(4, 0x80),                /* x4 = 0x80000 */
        enc_addi(4, 4, 1),               /* x4 = 0x80001 */
        enc_slli(4, 4, 12),              /* x4 = 0x80001000 (.tohost) */
        enc_addi(5, 0, (int)(code * 2 + 1)), /* x5 = HTIF exit payload */
        enc_sd(5, 4, 0),                 /* tohost = payload */
        0x0000006fu,                     /* j . */
    };
    size_t code_size = sizeof(instructions);
    size_t file_data_size = FILE_DATA_SIZE;
    size_t section_table = (SEGMENT_OFFSET + file_data_size + 7u) & ~(size_t)7u;
    const char names[] = "\0.tohost\0.shstrtab\0";
    size_t names_size = sizeof(names);
    size_t string_table = section_table + 3u * 64u;
    size_t elf_size = string_table + names_size;
    uint8_t *elf;
    uint8_t *segment;
    FILE *file;
    size_t i;

    if (code_size > file_data_size) {
        return -1;
    }
    elf = calloc(1, elf_size);
    if (elf == NULL) {
        return -1;
    }
    memcpy(elf, "\x7f" "ELF", 4);
    elf[4] = 2;  /* ELFCLASS64 */
    elf[5] = 1;  /* little endian */
    elf[6] = 1;  /* version */
    put_u16(elf, 16, 2);                  /* ET_EXEC */
    put_u16(elf, 18, 243);                /* EM_RISCV */
    put_u32(elf, 20, 1);                  /* version */
    put_u64(elf, 24, BASE);               /* entry */
    put_u64(elf, 32, 64);                 /* program header offset */
    put_u64(elf, 40, section_table);      /* section header offset */
    put_u16(elf, 52, 64);                 /* ELF header size */
    put_u16(elf, 54, 56);                 /* program header size */
    put_u16(elf, 56, 1);                  /* program header count */
    put_u16(elf, 58, 64);                 /* section header size */
    put_u16(elf, 60, 3);                  /* section count */
    put_u16(elf, 62, 2);                  /* shstrtab index */

    put_u32(elf, 64, 1);                  /* PT_LOAD */
    put_u32(elf, 68, 7);                  /* R|W|X */
    put_u64(elf, 72, SEGMENT_OFFSET);     /* p_offset */
    put_u64(elf, 80, BASE);               /* p_vaddr */
    put_u64(elf, 88, BASE);               /* p_paddr */
    put_u64(elf, 96, file_data_size);     /* p_filesz */
    put_u64(elf, 104, MEMORY_SIZE);       /* p_memsz */
    put_u64(elf, 112, 0x1000);            /* p_align */

    segment = elf + SEGMENT_OFFSET;
    for (i = 0; i < code_size / sizeof(instructions[0]); i += 1) {
        put_u32(segment, i * 4, instructions[i]);
    }

    put_section(elf, section_table + 64, 1, 1, 3, TOHOST,
                SEGMENT_OFFSET + TOHOST_OFFSET, 8, 8);
    put_section(elf, section_table + 128, 9, 3, 0, 0, string_table,
                names_size, 1);
    memcpy(elf + string_table, names, names_size);

    file = fopen(path, "wb");
    if (file == NULL) {
        free(elf);
        return -1;
    }
    if (fwrite(elf, 1, elf_size, file) != elf_size) {
        fclose(file);
        free(elf);
        return -1;
    }
    fclose(file);
    free(elf);
    return 0;
}

/* ------------------------------------------------------------ test target */

typedef struct test_target {
    uint8_t ram[RAM_SIZE];
    int32_t status;
    int unknown;
    int reset_ack;
    int reset_acks;
    int transactions;
    int attempt_reentry;
    int32_t reentry_status;
    ruscv_machine *handle;
} test_target;

static void target_b_transport(void *user, ruscv_payload *payload)
{
    test_target *target = user;
    size_t offset = (size_t)(payload->address - BASE);
    size_t length = payload->length;

    target->transactions += 1;
    if (target->attempt_reentry && target->reentry_status == 0) {
        ruscv_boundary ignored;
        /* A callback runs inside a ruscv-sim call, so this must be refused. */
        target->reentry_status = ruscv_machine_boundary(target->handle, &ignored);
    }
    if (offset + length > RAM_SIZE) {
        payload->status = RUSCV_TLM_ADDRESS_ERROR;
        return;
    }
    if (payload->command == RUSCV_COMMAND_READ) {
        memcpy(payload->data, target->ram + offset, length);
    } else {
        memcpy(target->ram + offset, payload->data, length);
    }
    payload->status = target->status;
    if (target->unknown) {
        payload->outcome_unknown = 1;
    }
}

static size_t target_transport_dbg(void *user, ruscv_debug *debug)
{
    test_target *target = user;
    size_t offset = (size_t)(debug->address - BASE);
    size_t length = debug->length;

    if (offset + length > RAM_SIZE) {
        return 0;
    }
    if (debug->command == RUSCV_COMMAND_READ) {
        memcpy(debug->data, target->ram + offset, length);
    } else {
        memcpy(target->ram + offset, debug->data, length);
    }
    return length;
}

static int32_t target_transport_reset(void *user)
{
    test_target *target = user;
    if (!target->reset_ack) {
        return 0;
    }
    target->reset_acks += 1;
    return 1;
}

static ruscv_target target_table(test_target *target, uint64_t max_delay_fs)
{
    ruscv_target table;
    memset(&table, 0, sizeof(table));
    table.max_delay_fs = max_delay_fs;
    table.user = target;
    table.b_transport = target_b_transport;
    table.transport_dbg = target_transport_dbg;
    table.transport_reset = target_transport_reset;
    return table;
}

/* ------------------------------------------------------------ smoke cases */

static ruscv_result result_of(ruscv_machine *handle);

/* Reports an expected guest exit, including the status and the diagnostic. */
static void check_exit(ruscv_machine *handle, int status, const char *what)
{
    ruscv_result run = result_of(handle);
    char diagnostic[256];
    int ok = status == RUSCV_OK && run.state == RUSCV_RUN_EXITED && run.exit_code == 2;

    if (!ok) {
        memset(diagnostic, 0, sizeof(diagnostic));
        ruscv_machine_diagnostic(handle, diagnostic, sizeof(diagnostic));
        printf("      status %d, state %d, code %u, diagnostic: %s\n", status, run.state,
               run.exit_code, diagnostic);
    }
    check(ok, what);
}

static void init_target(test_target *target, ruscv_machine *handle, int attempt_reentry)
{
    memset(target, 0, sizeof(*target));
    target->status = RUSCV_TLM_OK;
    target->reset_ack = 1;
    target->handle = handle;
    target->attempt_reentry = attempt_reentry;
    target->reentry_status = 0;
}

static ruscv_machine *create_machine(const ruscv_target *table, int32_t isa,
                                     uint64_t period_fs, int *status)
{
    ruscv_machine *handle = NULL;
    *status = ruscv_machine_create(table, isa, period_fs, &handle);
    return handle;
}

static int grant(ruscv_machine *handle, uint64_t start, uint64_t end,
                 uint64_t budget, ruscv_grant *out)
{
    memset(out, 0, sizeof(*out));
    return ruscv_machine_run_grant(handle, start, end, budget, out);
}

static ruscv_result result_of(ruscv_machine *handle)
{
    ruscv_result result;
    memset(&result, 0, sizeof(result));
    ruscv_machine_result(handle, &result);
    return result;
}

struct foreign_call {
    ruscv_machine *handle;
    int32_t status;
};

static void *foreign_thread(void *argument)
{
    struct foreign_call *call = argument;
    ruscv_boundary facts;

    memset(&facts, 0, sizeof(facts));
    call->status = ruscv_machine_boundary(call->handle, &facts);
    return NULL;
}

static void wrong_thread_case(ruscv_machine *handle)
{
    struct foreign_call call;
    pthread_t thread;

    call.handle = handle;
    call.status = RUSCV_OK;
    if (pthread_create(&thread, NULL, foreign_thread, &call) != 0) {
        check(0, "wrong-thread call was rejected");
        return;
    }
    pthread_join(thread, NULL);
    check(call.status == RUSCV_ERROR_NOT_CREATOR_THREAD,
          "wrong-thread call was rejected");
}

static void lifecycle_and_profile_case(const char *guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    ruscv_result run;
    int status;

    init_target(target, NULL, 0);
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    check(status == RUSCV_OK && handle != NULL, "create without an image");
    if (handle == NULL) {
        free(target);
        return;
    }
    target->handle = handle;

    {
        ruscv_boundary facts;
        memset(&facts, 0, sizeof(facts));
        check(ruscv_machine_boundary(handle, &facts) == RUSCV_ERROR_NO_IMAGE,
              "a call before load reports no image");
    }
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK, "load a compressed guest");
    status = grant(handle, 0, RUSCV_TURNS_UNBOUNDED, RUSCV_TURNS_UNBOUNDED, &result);
    check(status == RUSCV_OK, "run a grant without a horizon");
    check_exit(handle, status, "rv64imafdc executes the compressed guest and reports its code");
    check(ruscv_machine_destroy(handle) == RUSCV_OK, "destroy a handle");

    /* The same image under rv64ima traps on the compressed first parcel. */
    init_target(target, NULL, 0);
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMA, TICK_FS, &status);
    check(status == RUSCV_OK, "create under rv64ima");
    target->handle = handle;
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK, "load under rv64ima");
    status = grant(handle, 0, 2000 * TICK_FS, 2000, &result);
    run = result_of(handle);
    check(status == RUSCV_OK && run.state != RUSCV_RUN_EXITED,
          "rv64ima does not execute an IALIGN=16 guest");
    ruscv_machine_destroy(handle);
    free(target);
}

static void grants_and_commit_log_case(const char *guest, const char *log_path)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant first;
    ruscv_grant second;
    ruscv_boundary facts;
    char *text;
    long size;
    FILE *file;
    int status;

    init_target(target, NULL, 0);
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(status == RUSCV_OK, "create for the grant cases");
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK, "load for the grant cases");
    check(ruscv_machine_set_commit_log(handle, log_path) == RUSCV_OK,
          "route the commit log to a path");

    status = grant(handle, 0, 1000 * TICK_FS, 1, &first);
    check(status == RUSCV_OK, "run a one-turn grant");
    check(first.committed_start_fs == 0 && first.consumed_fs == TICK_FS,
          "the grant reports the time it consumed");
    check(first.committed_end_fs == first.committed_start_fs + first.consumed_fs,
          "the committed end equals start plus consumed");
    check(first.boundary_reached && first.facts.budget_exhausted,
          "a one-turn budget reaches the grant boundary");
    check(first.facts.transport_delay_fs == 0, "a zero-delay target reports no delay");

    status = grant(handle, first.committed_end_fs,
                   first.committed_end_fs + 1000 * TICK_FS, 4, &second);
    check(status == RUSCV_OK && second.facts.turns == 5,
          "the second grant starts at the committed time and counts turns");
    check(second.consumed_fs == 4 * TICK_FS, "four zero-delay turns consume four ticks");

    {
        uint64_t committed = second.committed_end_fs;

        status = grant(handle, committed - TICK_FS, UINT64_MAX / 2, 4, &second);
        check(status == RUSCV_ERROR_TIME_AUTHORITY,
              "a start earlier than the committed time is rejected");
        /* The caller's own clock may not rewind either, even when the
         * Machine's cursor is far behind it. */
        {
            ruscv_machine *ahead = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
            ruscv_grant lead;

            check(ahead != NULL && status == RUSCV_OK, "create a caller-clock handle");
            if (ahead != NULL) {
                target->handle = ahead;
                check(ruscv_machine_load_elf(ahead, guest) == RUSCV_OK,
                      "load for the caller-clock case");
                status = grant(ahead, 100 * TICK_FS, 110 * TICK_FS, 1, &lead);
                check(status == RUSCV_OK && lead.consumed_fs == TICK_FS,
                      "a grant far ahead of the Machine cursor runs");
                status = grant(ahead, 50 * TICK_FS, 200 * TICK_FS, 1, &lead);
                check(status == RUSCV_ERROR_TIME_AUTHORITY,
                      "a start earlier than the caller's committed time is rejected");
                ruscv_machine_destroy(ahead);
            }
        }
        memset(&facts, 0, sizeof(facts));
        ruscv_machine_boundary(handle, &facts);
        check(facts.turns == 5, "the rejected grant did not rewind the cursor");
        status = grant(handle, committed, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &second);
        check_exit(handle, status, "the remaining grant reaches the guest exit");
    }

    file = fopen(log_path, "rb");
    check(file != NULL, "the commit log exists");
    if (file != NULL) {
        fseek(file, 0, SEEK_END);
        size = ftell(file);
        fseek(file, 0, SEEK_SET);
        text = calloc(1, (size_t)size + 1);
        if (text != NULL && size > 0) {
            if (fread(text, 1, (size_t)size, file) == (size_t)size) {
                text[size] = 0;
            }
        }
        fclose(file);
        check(text != NULL && strstr(text, "core   0: 3 0x0000000080000000") != NULL,
              "the commit log starts at the entry PC");
        free(text);
    }

    ruscv_machine_destroy(handle);
    free(target);
}

static void reentry_case(const char *guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    ruscv_result run;
    int status;

    init_target(target, NULL, 1);
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(status == RUSCV_OK, "create for the re-entry case");
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK, "load for the re-entry case");
    status = grant(handle, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    run = result_of(handle);
    check(status == RUSCV_OK, "the outer grant still runs");
    check(target->reentry_status == RUSCV_ERROR_REENTRANT,
          "a callback that re-enters the C ABI is rejected");
    check(run.state == RUSCV_RUN_EXITED && run.exit_code == 2,
          "a rejected re-entry does not disturb the outer run");
    ruscv_machine_destroy(handle);
    free(target);
}

static void panic_case(const char *guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_boundary facts;
    ruscv_grant result;
    int status;

    init_target(target, NULL, 0);
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(status == RUSCV_OK, "create for the panic case");
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK, "load for the panic case");
    check(ruscv_machine_self_test_panic(handle) == RUSCV_ERROR_PANIC,
          "a Rust panic is caught at the boundary");
    memset(&facts, 0, sizeof(facts));
    check(ruscv_machine_boundary(handle, &facts) == RUSCV_ERROR_POISONED,
          "a caught panic poisons the handle");
    check(ruscv_machine_run_grant(handle, 0, UINT64_MAX / 2, 1, &result) ==
              RUSCV_ERROR_POISONED,
          "a poisoned handle refuses grants");
    check(ruscv_machine_destroy(handle) == RUSCV_OK, "a poisoned handle can be destroyed");
    free(target);
}

static void unknown_completion_case(const char *guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    ruscv_result run;
    int status;
    int transactions;

    init_target(target, NULL, 0);
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(status == RUSCV_OK, "create for the unknown-completion case");
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK,
          "load for the unknown-completion case");
    target->unknown = 1;
    status = grant(handle, 0, UINT64_MAX / 2, 100, &result);
    run = result_of(handle);
    check(status == RUSCV_OK && result.facts.uncertain,
          "an unknown completion is reported, not hidden");
    check(run.state == RUSCV_RUN_FAILED, "an unknown completion is not a passing result");
    transactions = target->transactions;
    status = grant(handle, 0, UINT64_MAX / 2, 100, &result);
    check(status == RUSCV_ERROR_UNKNOWN_COMPLETION,
          "no further grant is accepted after an unknown completion");
    check(target->transactions == transactions,
          "no transaction reaches the poisoned transport");
    /* The target has quiesced, so it no longer reports an unknown outcome. */
    target->unknown = 0;
    check(ruscv_machine_reset(handle) == RUSCV_OK,
          "reset is the acknowledged recovery path");
    check(target->reset_acks > 0, "reset obtained the transport-reset acknowledgement");
    status = grant(handle, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    check_exit(handle, status, "the reset domain runs to the guest exit");
    ruscv_machine_destroy(handle);
    free(target);
}

static void transport_status_case(const char *guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    ruscv_result run;
    char diagnostic[256];
    int status;

    init_target(target, NULL, 0);
    target->status = RUSCV_TLM_GENERIC_ERROR;
    table = target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(status == RUSCV_OK, "create for the transport-status case");
    check(ruscv_machine_load_elf(handle, guest) == RUSCV_OK,
          "load for the transport-status case");
    status = grant(handle, 0, UINT64_MAX / 2, 100, &result);
    run = result_of(handle);
    check(status == RUSCV_OK && result.facts.failed,
          "TLM_GENERIC_ERROR_RESPONSE is a host failure");
    check(run.state == RUSCV_RUN_FAILED && run.exit_code == 0,
          "a transport status is not a passing ruscv-sim result");
    memset(diagnostic, 0, sizeof(diagnostic));
    check(ruscv_machine_diagnostic(handle, diagnostic, sizeof(diagnostic)) > 0 &&
              strstr(diagnostic, "TLM_GENERIC_ERROR_RESPONSE") != NULL,
          "the transport failure keeps its diagnostic");
    ruscv_machine_destroy(handle);
    free(target);
}

static void header_case(void)
{
    ruscv_machine *handle = NULL;
    ruscv_target table;
    test_target *target = calloc(1, sizeof(test_target));

    memset(&table, 0, sizeof(table));
    check(ruscv_machine_create(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &handle) ==
              RUSCV_ERROR_INVALID_ARGUMENT,
          "a target without callbacks is rejected");
    check(handle == NULL, "a rejected create publishes no handle");
    check(ruscv_machine_create(NULL, 9, TICK_FS, &handle) == RUSCV_ERROR_INVALID_ARGUMENT,
          "an unknown profile code is rejected");
    check(ruscv_machine_create(NULL, RUSCV_ISA_RV64IMA, 0, &handle) ==
              RUSCV_ERROR_INVALID_ARGUMENT,
          "a zero tick period is rejected");
    init_target(target, NULL, 0);
    table = target_table(target, UINT64_MAX);
    check(ruscv_machine_create(&table, RUSCV_ISA_RV64IMA, 1, &handle) == RUSCV_ERROR_CONFIG,
          "an unrepresentable declared maximum is a configuration failure");
    check(ruscv_machine_destroy(NULL) == RUSCV_OK, "destroying NULL is harmless");
    free(target);
}

int main(int argc, char **argv)
{
    const char *work = argc > 1 ? argv[1] : ".";
    char guest[4096];
    char log_path[4096];
    ruscv_machine *handle = NULL;
    int status;

    if (snprintf(guest, sizeof(guest), "%s/capi-smoke-guest.elf", work) >=
            (int)sizeof(guest) ||
        snprintf(log_path, sizeof(log_path), "%s/capi-smoke-commits.log", work) >=
            (int)sizeof(log_path)) {
        fprintf(stderr, "work path too long\n");
        return 2;
    }
    if (build_guest(guest, 2) != 0) {
        fprintf(stderr, "cannot write the guest fixture\n");
        return 2;
    }

    header_case();
    lifecycle_and_profile_case(guest);
    grants_and_commit_log_case(guest, log_path);
    reentry_case(guest);
    panic_case(guest);
    unknown_completion_case(guest);
    transport_status_case(guest);

    /* The wrong-thread rejection needs a live handle of its own. */
    handle = create_machine(NULL, RUSCV_ISA_RV64IMA, TICK_FS, &status);
    check(status == RUSCV_OK, "create a native handle for the thread check");
    if (handle != NULL) {
        wrong_thread_case(handle);
        ruscv_machine_destroy(handle);
    }

    if (failures == 0) {
        printf("C ABI smoke: PASS\n");
        return 0;
    }
    printf("C ABI smoke: FAIL (%d checks)\n", failures);
    return 1;
}
