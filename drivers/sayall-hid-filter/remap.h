// SPDX-License-Identifier: MIT
// Adapted from QL-4/RemoteMapper, be8b57330c26a70d8b8ec9ff1e60c23251a2fc31.
#pragma once

#include <ntddk.h>

// Only the first key slot of RC003 keyboard report 1 is rewritten. The
// device-specific INF is the identity boundary; this function has no state.
BOOLEAN
SayAllRemapReport(
    _Inout_updates_bytes_(Length) PUCHAR Report,
    _In_ SIZE_T Length
    );
