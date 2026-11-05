/*
 * Plain-C compile-and-link smoke for the ruscv-sim C ABI (dev-plan D8/D11).
 *
 * The script `scripts/test_capi_smoke.sh` builds the static library with the
 * `capi` feature, compiles this file with the host C compiler and runs it.  It
 * covers the U5 acceptance surface: lifecycle, profile selection, commit-log
 * output, grant time fields and earlier-start rejection, wrong-thread
 * rejection, re-entry rejection, panic poisoning, a transport status that is
 * not a passing result, an unknown completion followed by reset, and the
 * direct-memory ABI: grant, NULL callback, denial, invalidation between grants
 * and inside a callback, wrong-thread rejection, the re-entry exception, and
 * reset restoring grants after a poisoned transport.
 *
 * The guest is a hand-built ELF fixture (the same shape as
 * `tests/common/public_elf.rs`) so the smoke needs only a host C compiler.
 */

#include "ruscv_sim.h"

#include <pthread.h>
#include <stdatomic.h>
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

static uint32_t enc_lw(int rd, int rs1, int imm)
{
    return ((uint32_t)imm & 0xfffu) << 20 | (uint32_t)rs1 << 15 | 2u << 12 |
           (uint32_t)rd << 7 | 0x03u;
}

/*
 * Writes one ELF64/RISC-V executable around `instructions`.
 *
 * The first parcel of every fixture is two compressed `c.nop`s, so the image
 * only executes under an IALIGN=16 profile: every profile without compression
 * traps on it.
 */
static int write_guest(const char *path, const uint32_t *instructions, size_t count)
{
    size_t code_size = count * sizeof(instructions[0]);
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
    for (i = 0; i < count; i += 1) {
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

/* A guest that reports HTIF exit `code` through the declared `.tohost`. */
static int build_guest(const char *path, uint32_t code)
{
    const uint32_t instructions[] = {
        0x00010001u,                     /* c.nop; c.nop (profile-sensitive) */
        enc_lui(4, 0x80),                /* x4 = 0x80000 */
        enc_addi(4, 4, 1),               /* x4 = 0x80001 */
        enc_slli(4, 4, 12),              /* x4 = 0x80001000 (.tohost) */
        enc_addi(5, 0, (int)(code * 2 + 1)), /* x5 = HTIF exit payload */
        enc_sd(5, 4, 0),                 /* tohost = payload */
        0x0000006fu,                     /* j . */
    };

    return write_guest(path, instructions, sizeof(instructions) / sizeof(instructions[0]));
}

/*
 * The same guest with one data load from `.tohost` between the code and the
 * exit store.
 *
 * With a code-page grant, that load is the single access outside the grant, so
 * it always takes one `b_transport`.  A test can invalidate the code grant from
 * inside that callback, and the fetch after it proves the invalidation reached
 * the adapter.  The store to `.tohost + 8` runs first and survives the exit
 * poll's clearing of the signal, so it is the byte a test can inspect.
 */
static int build_direct_guest(const char *path, uint32_t code)
{
    const uint32_t instructions[] = {
        0x00010001u,                     /* c.nop; c.nop (profile-sensitive) */
        enc_lui(4, 0x80),                /* x4 = 0x80000 */
        enc_addi(4, 4, 1),               /* x4 = 0x80001 */
        enc_slli(4, 4, 12),              /* x4 = 0x80001000 (.tohost) */
        enc_lw(6, 4, 0),                 /* data read outside the code grant */
        enc_addi(5, 0, (int)(code * 2 + 1)), /* x5 = HTIF exit payload */
        enc_sd(5, 4, 8),                 /* witness store, outside .tohost */
        enc_sd(5, 4, 0),                 /* tohost = payload */
        0x0000006fu,                     /* j . */
    };

    return write_guest(path, instructions, sizeof(instructions) / sizeof(instructions[0]));
}

/* ------------------------------------------------------------ test target */

typedef struct test_target {
    /* A direct-memory grant promises relaxed byte-wise atomic access
     * (direct-memory D10). */
    _Atomic unsigned char ram[RAM_SIZE];
    int32_t status;
    int unknown;
    int reset_ack;
    int reset_acks;
    int transactions;
    int attempt_reentry;
    int32_t reentry_status;
    ruscv_machine *handle;
    /* Direct memory: hint on completed non-atomic transactions. */
    int hint;
    /* Inclusive grant window; queries outside it are denied. */
    int grant_enabled;
    uint64_t grant_start;
    uint64_t grant_end;
    int queries;
    /* Invalidate the whole space from inside the `.tohost` load. */
    int invalidate_in_read;
    int invalidations;
    int32_t invalidate_status;
    /* Mark the `.tohost` load outcome-unknown. */
    int unknown_on_data_read;
} test_target;

/* The target's storage is atomic, so every access goes through these. */
static void ram_read(test_target *target, size_t offset, uint8_t *bytes, size_t length)
{
    size_t i;

    for (i = 0; i < length; i += 1) {
        bytes[i] = atomic_load_explicit(&target->ram[offset + i], memory_order_relaxed);
    }
}

static void ram_write(test_target *target, size_t offset, const uint8_t *bytes,
                      size_t length)
{
    size_t i;

    for (i = 0; i < length; i += 1) {
        atomic_store_explicit(&target->ram[offset + i], bytes[i], memory_order_relaxed);
    }
}

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
        ram_read(target, offset, payload->data, length);
    } else {
        ram_write(target, offset, payload->data, length);
    }
    payload->status = target->status;
    if (target->unknown ||
        (target->unknown_on_data_read && payload->access_kind == RUSCV_ACCESS_DATA_READ)) {
        payload->outcome_unknown = 1;
    }
    if (target->hint && payload->status == RUSCV_TLM_OK && payload->atomic.present == 0) {
        payload->dmi_allowed = 1;
    }
    if (target->invalidate_in_read && target->invalidations == 0 &&
        payload->access_kind == RUSCV_ACCESS_DATA_READ && payload->atomic.present == 0) {
        /* The one ABI entry a callback may call: accepted even though the
         * outer call still holds the Machine's run-state lock. */
        target->invalidations += 1;
        target->invalidate_status =
            ruscv_machine_invalidate_direct_mem(target->handle, 0, UINT64_MAX);
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
        ram_read(target, offset, debug->data, length);
    } else {
        ram_write(target, offset, debug->data, length);
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

static int32_t target_get_direct_mem_ptr(void *user, ruscv_direct_mem *direct_mem)
{
    test_target *target = user;

    target->queries += 1;
    if (!target->grant_enabled || direct_mem->address < target->grant_start ||
        direct_mem->address > target->grant_end) {
        return RUSCV_DMI_DENIED;
    }
    direct_mem->start = target->grant_start;
    direct_mem->end = target->grant_end;
    direct_mem->pointer = (uint8_t *)&target->ram[target->grant_start - BASE];
    direct_mem->access = RUSCV_DMI_ACCESS_READ_WRITE;
    direct_mem->read_latency_fs = 0;
    direct_mem->write_latency_fs = 0;
    return RUSCV_DMI_GRANTED;
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

/* The same table with the non-NULL query that selects direct memory
 * (direct-memory D12). */
static ruscv_target direct_target_table(test_target *target, uint64_t max_delay_fs)
{
    ruscv_target table = target_table(target, max_delay_fs);
    table.get_direct_mem_ptr = target_get_direct_mem_ptr;
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
    size_t i;

    memset(target, 0, sizeof(*target));
    for (i = 0; i < RAM_SIZE; i += 1) {
        atomic_init(&target->ram[i], 0);
    }
    target->status = RUSCV_TLM_OK;
    target->reset_ack = 1;
    target->handle = handle;
    target->attempt_reentry = attempt_reentry;
    target->reentry_status = 0;
    target->invalidate_status = RUSCV_OK;
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

struct foreign_invalidate {
    ruscv_machine *handle;
    int32_t status;
};

static void *foreign_invalidate_thread(void *argument)
{
    struct foreign_invalidate *call = argument;

    call->status = ruscv_machine_invalidate_direct_mem(call->handle, 0, UINT64_MAX);
    return NULL;
}

static void invalidate_on_another_thread_case(ruscv_machine *handle)
{
    struct foreign_invalidate call;
    pthread_t thread;

    call.handle = handle;
    call.status = RUSCV_OK;
    if (pthread_create(&thread, NULL, foreign_invalidate_thread, &call) != 0) {
        check(0, "a wrong-thread invalidation was rejected");
        return;
    }
    pthread_join(thread, NULL);
    check(call.status == RUSCV_ERROR_NOT_CREATOR_THREAD,
          "a wrong-thread invalidation was rejected");
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

/* --------------------------------------------------- direct-memory cases */

/* The code page above the first two parcels: the `.tohost` load is outside it,
 * so that load always takes one `b_transport`. */
#define CODE_GRANT_START (BASE + 8u)
#define CODE_GRANT_END (BASE + 0xfffu)

static void direct_memory_grant_case(const char *direct_guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    uint8_t witness = 0;
    int status;

    init_target(target, NULL, 0);
    target->hint = 1;
    target->grant_enabled = 1;
    target->grant_start = BASE;
    target->grant_end = BASE + RAM_SIZE - 1;
    table = direct_target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(status == RUSCV_OK && handle != NULL, "create a direct-memory handle");
    if (handle == NULL) {
        free(target);
        return;
    }
    check(ruscv_machine_load_elf(handle, direct_guest) == RUSCV_OK,
          "load the direct-memory guest");
    status = grant(handle, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    check_exit(handle, status, "the direct-memory grant serves the guest to its exit");
    check(target->queries == 1, "one hint is exactly one direct-memory query");
    check(target->transactions == 1,
          "only the first uncovered fetch reached b_transport");
    ram_read(target, (size_t)((TOHOST + 8) - BASE), &witness, 1);
    check(witness == 5, "the direct store reached the target's own bytes");
    ruscv_machine_destroy(handle);
    free(target);
}

/* One blocking cost with a NULL callback, a denial, and a plain target. */
static int blocking_transactions(const char *guest, int with_callback, int with_hint)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    int status;
    int transactions = -1;

    init_target(target, NULL, 0);
    target->hint = with_hint;
    table = with_callback ? direct_target_table(target, 0) : target_table(target, 0);
    if (with_callback) {
        target->grant_enabled = 0;
    }
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    if (handle == NULL || status != RUSCV_OK) {
        free(target);
        return -1;
    }
    if (ruscv_machine_load_elf(handle, guest) != RUSCV_OK ||
        grant(handle, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result) != RUSCV_OK ||
        result_of(handle).state != RUSCV_RUN_EXITED) {
        ruscv_machine_destroy(handle);
        free(target);
        return -1;
    }
    transactions = target->transactions;
    if (with_callback) {
        check(target->queries > 0, "a denial answered at least one query");
    } else {
        check(target->queries == 0, "a NULL callback issued no query");
    }
    ruscv_machine_destroy(handle);
    free(target);
    return transactions;
}

static void direct_memory_off_case(const char *guest)
{
    int blocking = blocking_transactions(guest, 0, 0);
    int hinted = blocking_transactions(guest, 0, 1);
    int denied = blocking_transactions(guest, 1, 1);

    check(blocking > 0, "the reference run executed transactions");
    check(hinted == blocking, "a NULL callback behaves exactly as direct memory off");
    check(denied == blocking, "a denial caches nothing and changes no transaction");
}

static void direct_memory_invalidation_between_grants_case(const char *direct_guest)
{
    test_target *invalidated_target = calloc(1, sizeof(test_target));
    test_target *baseline_target = calloc(1, sizeof(test_target));
    ruscv_target invalidated_table;
    ruscv_target baseline_table;
    ruscv_machine *invalidated;
    ruscv_machine *baseline;
    ruscv_grant first;
    ruscv_grant after;
    int status;

    init_target(invalidated_target, NULL, 0);
    invalidated_target->hint = 1;
    invalidated_target->grant_enabled = 1;
    invalidated_target->grant_start = CODE_GRANT_START;
    invalidated_target->grant_end = CODE_GRANT_END;
    init_target(baseline_target, NULL, 0);
    baseline_target->hint = 1;
    baseline_target->grant_enabled = 1;
    baseline_target->grant_start = CODE_GRANT_START;
    baseline_target->grant_end = CODE_GRANT_END;
    invalidated_table = direct_target_table(invalidated_target, 0);
    baseline_table = direct_target_table(baseline_target, 0);
    invalidated = create_machine(&invalidated_table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    invalidated_target->handle = invalidated;
    baseline = create_machine(&baseline_table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    baseline_target->handle = baseline;
    check(invalidated != NULL && baseline != NULL,
          "create the invalidation-between-grants handles");
    if (invalidated == NULL || baseline == NULL) {
        ruscv_machine_destroy(invalidated);
        ruscv_machine_destroy(baseline);
        free(invalidated_target);
        free(baseline_target);
        return;
    }
    check(ruscv_machine_load_elf(invalidated, direct_guest) == RUSCV_OK &&
              ruscv_machine_load_elf(baseline, direct_guest) == RUSCV_OK,
          "load the invalidation-between-grants guest");

    /* Four turns reach the first granted fetch, so a grant is cached. */
    check(grant(invalidated, 0, UINT64_MAX / 2, 4, &first) == RUSCV_OK &&
              invalidated_target->queries > 0,
          "the four-turn grant cached a grant");
    check(grant(baseline, 0, UINT64_MAX / 2, 4, &first) == RUSCV_OK,
          "the baseline four-turn grant ran");
    check(ruscv_machine_invalidate_direct_mem(invalidated, 0, UINT64_MAX) == RUSCV_OK,
          "an invalidation between grants is accepted");
    check(invalidated_target->transactions == baseline_target->transactions,
          "an invalidation itself touches no target");

    /* The next fetch takes b_transport in the invalidated run and not in the
     * baseline, so the difference is exactly the removed grant. */
    check(grant(invalidated, first.committed_end_fs, UINT64_MAX / 2, 1, &after) == RUSCV_OK,
          "the one-turn grant after the invalidation ran");
    check(invalidated_target->transactions == baseline_target->transactions + 1,
          "the fetch after an invalidation took exactly one b_transport");
    check(grant(baseline, first.committed_end_fs, UINT64_MAX / 2, 1, &after) == RUSCV_OK,
          "the baseline completed the same single turn");
    check(baseline_target->transactions + 1 == invalidated_target->transactions,
          "the baseline grant stayed direct");
    status = grant(invalidated, after.committed_end_fs, UINT64_MAX / 2,
                   RUSCV_TURNS_UNBOUNDED, &after);
    check_exit(invalidated, status, "the invalidated run still reaches the guest exit");
    ruscv_machine_destroy(invalidated);
    ruscv_machine_destroy(baseline);
    free(invalidated_target);
    free(baseline_target);
}

static void direct_memory_wrong_thread_case(const char *direct_guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant first;
    ruscv_grant next;
    int status;
    int transactions;

    init_target(target, NULL, 0);
    target->hint = 1;
    target->grant_enabled = 1;
    target->grant_start = CODE_GRANT_START;
    target->grant_end = CODE_GRANT_END;
    table = direct_target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(handle != NULL && status == RUSCV_OK,
          "create the wrong-thread invalidation handle");
    if (handle == NULL) {
        free(target);
        return;
    }
    check(ruscv_machine_load_elf(handle, direct_guest) == RUSCV_OK,
          "load the wrong-thread invalidation guest");
    check(grant(handle, 0, UINT64_MAX / 2, 4, &first) == RUSCV_OK &&
              target->queries > 0,
          "the wrong-thread case cached a grant");
    transactions = target->transactions;
    invalidate_on_another_thread_case(handle);
    check(grant(handle, first.committed_end_fs, UINT64_MAX / 2, 1, &next) == RUSCV_OK,
          "the grant after the rejected invalidation ran");
    check(target->transactions == transactions,
          "the rejected invalidation removed the cached grant");
    ruscv_machine_destroy(handle);
    free(target);
}

static void direct_memory_invalidate_in_callback_case(const char *direct_guest)
{
    test_target *invalidated_target = calloc(1, sizeof(test_target));
    test_target *baseline_target = calloc(1, sizeof(test_target));
    ruscv_target invalidated_table;
    ruscv_target baseline_table;
    ruscv_machine *invalidated;
    ruscv_machine *baseline;
    ruscv_grant result;
    int status;

    init_target(invalidated_target, NULL, 0);
    invalidated_target->hint = 1;
    invalidated_target->grant_enabled = 1;
    invalidated_target->grant_start = CODE_GRANT_START;
    invalidated_target->grant_end = CODE_GRANT_END;
    invalidated_target->invalidate_in_read = 1;
    init_target(baseline_target, NULL, 0);
    baseline_target->hint = 1;
    baseline_target->grant_enabled = 1;
    baseline_target->grant_start = CODE_GRANT_START;
    baseline_target->grant_end = CODE_GRANT_END;
    invalidated_table = direct_target_table(invalidated_target, 0);
    baseline_table = direct_target_table(baseline_target, 0);
    invalidated = create_machine(&invalidated_table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    invalidated_target->handle = invalidated;
    baseline = create_machine(&baseline_table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    baseline_target->handle = baseline;
    check(invalidated != NULL && baseline != NULL,
          "create the in-callback invalidation handles");
    if (invalidated == NULL || baseline == NULL) {
        ruscv_machine_destroy(invalidated);
        ruscv_machine_destroy(baseline);
        free(invalidated_target);
        free(baseline_target);
        return;
    }
    check(ruscv_machine_load_elf(invalidated, direct_guest) == RUSCV_OK &&
              ruscv_machine_load_elf(baseline, direct_guest) == RUSCV_OK,
          "load the in-callback invalidation guest");

    status = grant(invalidated, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    check_exit(invalidated, status, "the in-callback invalidation run reaches the exit");
    check(invalidated_target->invalidate_status == RUSCV_OK,
          "the invalidation entry is accepted inside b_transport");
    check(invalidated_target->invalidations == 1,
          "the in-callback invalidation ran exactly once");
    status = grant(baseline, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    check_exit(baseline, status, "the baseline run reaches the exit");
    check(invalidated_target->transactions == baseline_target->transactions + 1,
          "the granted fetch after the in-callback invalidation took b_transport");
    ruscv_machine_destroy(invalidated);
    ruscv_machine_destroy(baseline);
    free(invalidated_target);
    free(baseline_target);
}

static void direct_memory_reentry_case(const char *direct_guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    ruscv_result run;
    int status;

    init_target(target, NULL, 1);
    target->hint = 1;
    target->grant_enabled = 1;
    target->grant_start = CODE_GRANT_START;
    target->grant_end = CODE_GRANT_END;
    target->invalidate_in_read = 1;
    table = direct_target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(handle != NULL && status == RUSCV_OK, "create the direct-memory re-entry handle");
    if (handle == NULL) {
        free(target);
        return;
    }
    check(ruscv_machine_load_elf(handle, direct_guest) == RUSCV_OK,
          "load the direct-memory re-entry guest");
    status = grant(handle, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    run = result_of(handle);
    check(status == RUSCV_OK, "the outer grant still runs");
    check(target->reentry_status == RUSCV_ERROR_REENTRANT,
          "every other ABI entry inside a callback is still rejected");
    check(target->invalidate_status == RUSCV_OK,
          "the invalidation entry is the sole callback exception");
    check(run.state == RUSCV_RUN_EXITED && run.exit_code == 2,
          "neither the accepted nor the rejected call disturbed the run");
    ruscv_machine_destroy(handle);
    free(target);
}

static void direct_memory_unknown_completion_case(const char *direct_guest)
{
    test_target *target = calloc(1, sizeof(test_target));
    ruscv_target table;
    ruscv_machine *handle;
    ruscv_grant result;
    ruscv_result run;
    int status;
    int transactions;

    init_target(target, NULL, 0);
    target->hint = 1;
    target->grant_enabled = 1;
    target->grant_start = CODE_GRANT_START;
    target->grant_end = CODE_GRANT_END;
    target->unknown_on_data_read = 1;
    table = direct_target_table(target, 0);
    handle = create_machine(&table, RUSCV_ISA_RV64IMAFDC, TICK_FS, &status);
    target->handle = handle;
    check(handle != NULL && status == RUSCV_OK,
          "create the direct-memory unknown-completion handle");
    if (handle == NULL) {
        free(target);
        return;
    }
    check(ruscv_machine_load_elf(handle, direct_guest) == RUSCV_OK,
          "load the direct-memory unknown-completion guest");
    status = grant(handle, 0, UINT64_MAX / 2, 100, &result);
    run = result_of(handle);
    check(status == RUSCV_OK && result.facts.uncertain,
          "an unknown completion with direct memory active is reported");
    check(run.state == RUSCV_RUN_FAILED,
          "an unknown completion with direct memory active is not a pass");
    check(target->queries > 0, "the failing run had cached a grant");
    transactions = target->transactions;
    status = grant(handle, 0, UINT64_MAX / 2, 100, &result);
    check(status == RUSCV_ERROR_UNKNOWN_COMPLETION,
          "no further grant is accepted after the unknown completion");
    check(target->transactions == transactions,
          "no transaction reaches the poisoned transport");
    check(ruscv_machine_invalidate_direct_mem(handle, 0, UINT64_MAX) == RUSCV_OK,
          "invalidation is accepted on a poisoned transport handle");

    target->unknown_on_data_read = 0;
    check(ruscv_machine_reset(handle) == RUSCV_OK,
          "reset is the acknowledged recovery path for direct memory");
    check(target->reset_acks > 0, "reset obtained the transport-reset acknowledgement");
    transactions = target->transactions;
    status = grant(handle, 0, UINT64_MAX / 2, RUSCV_TURNS_UNBOUNDED, &result);
    check_exit(handle, status, "the reset domain runs to the guest exit");
    check(target->queries > 0, "reset restored the direct-memory grant path");
    check(target->transactions - transactions <
              blocking_transactions(direct_guest, 0, 0),
          "the post-reset run used direct memory again");
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
    /* Published x86-64 layout: the direct-memory additions are final fields, so
     * no earlier offset moved (see the header's layout rule). */
    check(offsetof(ruscv_target, transport_reset) == 32 &&
              offsetof(ruscv_target, get_direct_mem_ptr) == 40 &&
              sizeof(ruscv_target) == 48,
          "get_direct_mem_ptr is the final ruscv_target field");
    check(offsetof(ruscv_payload, atomic) == 56 &&
              offsetof(ruscv_payload, dmi_allowed) == 240 &&
              sizeof(ruscv_payload) == 248,
          "dmi_allowed is the final ruscv_payload field");
    free(target);
}

int main(int argc, char **argv)
{
    const char *work = argc > 1 ? argv[1] : ".";
    char guest[4096];
    char direct_guest[4096];
    char log_path[4096];
    ruscv_machine *handle = NULL;
    int status;

    if (snprintf(guest, sizeof(guest), "%s/capi-smoke-guest.elf", work) >=
            (int)sizeof(guest) ||
        snprintf(direct_guest, sizeof(direct_guest), "%s/capi-smoke-direct-guest.elf",
                 work) >= (int)sizeof(direct_guest) ||
        snprintf(log_path, sizeof(log_path), "%s/capi-smoke-commits.log", work) >=
            (int)sizeof(log_path)) {
        fprintf(stderr, "work path too long\n");
        return 2;
    }
    if (build_guest(guest, 2) != 0 || build_direct_guest(direct_guest, 2) != 0) {
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
    direct_memory_grant_case(direct_guest);
    direct_memory_off_case(direct_guest);
    direct_memory_invalidation_between_grants_case(direct_guest);
    direct_memory_wrong_thread_case(direct_guest);
    direct_memory_invalidate_in_callback_case(direct_guest);
    direct_memory_reentry_case(direct_guest);
    direct_memory_unknown_completion_case(direct_guest);

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
