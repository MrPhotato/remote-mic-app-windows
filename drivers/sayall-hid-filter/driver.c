// SPDX-License-Identifier: MIT
// Copyright (c) 2026 QL-4
// Adapted from QL-4/RemoteMapper, be8b57330c26a70d8b8ec9ff1e60c23251a2fc31.
#include "driver.h"
#include "remap.h"
#include <TraceLoggingProvider.h>

// Only counters, branch names, and NTSTATUS values are emitted. No device
// paths, addresses, report payloads, or audio enter this ETW provider.
TRACELOGGING_DEFINE_PROVIDER(
    g_SayAllHidProvider,
    "SayAll.HidFilter",
    (0x0aeb591f, 0x0c3f, 0x4043, 0xa6, 0x27, 0xfe, 0x28, 0x35, 0x6d, 0x69, 0xf9));

typedef struct _SAYALL_READ_COUNTERS {
    volatile LONG64 Completed;
    volatile LONG64 SendFailed;
    volatile LONG64 LowerFailed;
    volatile LONG64 Cancelled;
    volatile LONG64 ShortReport;
    volatile LONG64 BufferFailed;
    volatile LONG64 LengthMismatch;
    volatile LONG64 PassedThrough;
    volatile LONG64 VolumeUp;
    volatile LONG64 VolumeDown;
    volatile LONG64 Back;
} SAYALL_READ_COUNTERS;

static SAYALL_READ_COUNTERS g_ReadCounters;

static BOOLEAN
SayAllShouldTraceCount(LONG64 Count)
{
    // Aggregate at powers of two rather than logging each input packet.
    ULONGLONG value = (ULONGLONG)Count;
    return value != 0 && (value & (value - 1)) == 0;
}

static VOID
SayAllTraceCounters(PCSTR Reason)
{
    TraceLoggingWrite(
        g_SayAllHidProvider,
        "ReadSummary",
        TraceLoggingString(Reason, "reason"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.Completed, 0, 0), "completed"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.SendFailed, 0, 0), "send_failed"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.LowerFailed, 0, 0), "lower_failed"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.Cancelled, 0, 0), "cancelled"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.ShortReport, 0, 0), "short_report"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.BufferFailed, 0, 0), "buffer_failed"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.LengthMismatch, 0, 0), "length_mismatch"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.PassedThrough, 0, 0), "passed_through"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.VolumeUp, 0, 0), "volume_up_rewritten"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.VolumeDown, 0, 0), "volume_down_rewritten"),
        TraceLoggingInt64(InterlockedCompareExchange64(&g_ReadCounters.Back, 0, 0), "back_rewritten"));
}

static VOID
SayAllTraceFailure(PCSTR Stage, NTSTATUS Status, LONG64 Count)
{
    if (SayAllShouldTraceCount(Count)) {
        TraceLoggingWrite(
            g_SayAllHidProvider,
            "ReadFailure",
            TraceLoggingString(Stage, "stage"),
            TraceLoggingHexInt32((ULONG)Status, "status"),
            TraceLoggingInt64(Count, "count"));
    }
}

#ifdef ALLOC_PRAGMA
#pragma alloc_text(INIT, DriverEntry)
#pragma alloc_text(PAGE, SayAllEvtDeviceAdd)
#pragma alloc_text(PAGE, SayAllEvtDriverUnload)
#endif

NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT DriverObject,
    _In_ PUNICODE_STRING RegistryPath
    )
{
    WDF_DRIVER_CONFIG config;
    NTSTATUS status;
    NTSTATUS traceStatus;

    // Logging is optional. A failed provider registration must not prevent
    // the remote's existing keyboard stack from loading.
    traceStatus = TraceLoggingRegister(g_SayAllHidProvider);
    WDF_DRIVER_CONFIG_INIT(&config, SayAllEvtDeviceAdd);
    config.EvtDriverUnload = SayAllEvtDriverUnload;
    status = WdfDriverCreate(
        DriverObject,
        RegistryPath,
        WDF_NO_OBJECT_ATTRIBUTES,
        &config,
        WDF_NO_HANDLE);

    TraceLoggingWrite(
        g_SayAllHidProvider,
        "DriverStart",
        TraceLoggingHexInt32((ULONG)status, "driver_status"),
        TraceLoggingHexInt32((ULONG)traceStatus, "trace_status"));
    if (!NT_SUCCESS(status)) {
        // EvtDriverUnload is not called when DriverEntry fails.
        TraceLoggingUnregister(g_SayAllHidProvider);
    }
    return status;
}

VOID
SayAllEvtDriverUnload(_In_ WDFDRIVER Driver)
{
    UNREFERENCED_PARAMETER(Driver);
    PAGED_CODE();
    // WDF unload follows device/request teardown; no completion can outlive
    // the provider or this module. There are no worker threads or timers.
    SayAllTraceCounters("driver_unload");
    TraceLoggingUnregister(g_SayAllHidProvider);
}

NTSTATUS
SayAllEvtDeviceAdd(
    _In_ WDFDRIVER Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
    )
{
    WDFDEVICE device;
    WDF_IO_QUEUE_CONFIG queueConfig;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(Driver);
    PAGED_CODE();
    WdfFdoInitSetFilter(DeviceInit);
    status = WdfDeviceCreate(&DeviceInit, WDF_NO_OBJECT_ATTRIBUTES, &device);
    if (!NT_SUCCESS(status)) {
        TraceLoggingWrite(
            g_SayAllHidProvider,
            "DeviceCreateFailed",
            TraceLoggingHexInt32((ULONG)status, "status"));
        return status;
    }

    // Every request type except reads is passed through by the framework.
    // Reads are never held in a driver-owned queue after forwarding.
    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(&queueConfig, WdfIoQueueDispatchParallel);
    queueConfig.EvtIoRead = SayAllEvtIoRead;
    status = WdfIoQueueCreate(device, &queueConfig, WDF_NO_OBJECT_ATTRIBUTES, WDF_NO_HANDLE);
    TraceLoggingWrite(
        g_SayAllHidProvider,
        "DeviceReady",
        TraceLoggingHexInt32((ULONG)status, "queue_status"));
    return status;
}

VOID
SayAllEvtIoRead(
    _In_ WDFQUEUE Queue,
    _In_ WDFREQUEST Request,
    _In_ size_t Length
    )
{
    WDFIOTARGET target;
    NTSTATUS status;
    BOOLEAN sent;

    UNREFERENCED_PARAMETER(Length);
    target = WdfDeviceGetIoTarget(WdfIoQueueGetDevice(Queue));
    WdfRequestFormatRequestUsingCurrentType(Request);
    WdfRequestSetCompletionRoutine(Request, SayAllReadCompletion, WDF_NO_CONTEXT);
    sent = WdfRequestSend(Request, target, WDF_NO_SEND_OPTIONS);
    if (!sent) {
        // On FALSE no completion callback owns the request. Complete exactly
        // once, including a cancellation that occurred before forwarding.
        status = WdfRequestGetStatus(Request);
        SayAllTraceFailure("send", status, InterlockedIncrement64(&g_ReadCounters.SendFailed));
        WdfRequestComplete(Request, status);
    }
    // A successful asynchronous send may already have completed inline.
    // Never access Request after WdfRequestSend returns TRUE.
}

VOID
SayAllReadCompletion(
    _In_ WDFREQUEST Request,
    _In_ WDFIOTARGET Target,
    _In_ PWDF_REQUEST_COMPLETION_PARAMS CompletionParams,
    _In_ WDFCONTEXT Context
    )
{
    NTSTATUS lowerStatus = CompletionParams->IoStatus.Status;
    ULONG_PTR information = CompletionParams->IoStatus.Information;
    PVOID buffer = NULL;
    SIZE_T bufferLength = 0;
    LONG64 completed;

    UNREFERENCED_PARAMETER(Target);
    UNREFERENCED_PARAMETER(Context);

    if (lowerStatus == STATUS_CANCELLED) {
        InterlockedIncrement64(&g_ReadCounters.Cancelled);
    } else if (!NT_SUCCESS(lowerStatus)) {
        SayAllTraceFailure("lower", lowerStatus, InterlockedIncrement64(&g_ReadCounters.LowerFailed));
    } else if (information < 4) {
        InterlockedIncrement64(&g_ReadCounters.ShortReport);
    } else {
        NTSTATUS bufferStatus = WdfRequestRetrieveOutputBuffer(Request, 4, &buffer, &bufferLength);
        if (!NT_SUCCESS(bufferStatus)) {
            SayAllTraceFailure("buffer", bufferStatus, InterlockedIncrement64(&g_ReadCounters.BufferFailed));
        } else if (information > bufferLength || bufferLength < 4) {
            // An inconsistent completion is left unchanged. Do not rewrite
            // using a length that the framework did not actually make valid.
            SayAllTraceFailure("length", STATUS_INVALID_BUFFER_SIZE,
                InterlockedIncrement64(&g_ReadCounters.LengthMismatch));
        } else {
            PUCHAR report = (PUCHAR)buffer;
            UCHAR sourceUsage = report[3];
            NT_ANALYSIS_ASSUME(information <= bufferLength);
            if (SayAllRemapReport(report, (SIZE_T)information)) {
                volatile LONG64 *counter;
                LONG64 remapped;
                switch (sourceUsage) {
                case 0x80: counter = &g_ReadCounters.VolumeUp; break;
                case 0x81: counter = &g_ReadCounters.VolumeDown; break;
                default: counter = &g_ReadCounters.Back; break;
                }
                remapped = InterlockedIncrement64(counter);
                if (SayAllShouldTraceCount(remapped)) {
                    SayAllTraceCounters("usage_rewritten");
                }
            } else {
                InterlockedIncrement64(&g_ReadCounters.PassedThrough);
            }
        }
    }

    completed = InterlockedIncrement64(&g_ReadCounters.Completed);
    if (SayAllShouldTraceCount(completed)) {
        SayAllTraceCounters("read_completed");
    }
    // Preserve lower status and actual transfer length even on cancellation,
    // failure, an inaccessible buffer, or an unrecognised report. No request
    // or buffer is retained or accessed after this terminal operation.
    WdfRequestCompleteWithInformation(Request, lowerStatus, information);
}
