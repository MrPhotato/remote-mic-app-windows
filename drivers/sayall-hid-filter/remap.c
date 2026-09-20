// SPDX-License-Identifier: MIT
// Copyright (c) 2026 QL-4
// Adapted from QL-4/RemoteMapper, be8b57330c26a70d8b8ec9ff1e60c23251a2fc31.
#include "remap.h"

BOOLEAN
SayAllRemapReport(
    _Inout_updates_bytes_(Length) PUCHAR Report,
    _In_ SIZE_T Length
    )
{
    if (Report == NULL || Length < 4 || Report[0] != 0x01) {
        return FALSE;
    }

    // Upstream RC003 capture: report ID, modifiers, reserved, first key usage.
    // HIDCLASS can pad this to 121 bytes. Preserve padding and every other
    // slot: there is no evidence for scanning vendor data or additional keys.
    switch (Report[3]) {
    case 0x80: // Keyboard Page Volume Up -> F13.
        Report[3] = 0x68;
        return TRUE;
    case 0x81: // Keyboard Page Volume Down -> F14.
        Report[3] = 0x69;
        return TRUE;
    case 0xF1: // RC003 Back -> F15.
        Report[3] = 0x6A;
        return TRUE;
    default:
        // Includes key-up (0), voice F5 (0x3E), and existing navigation keys.
        return FALSE;
    }
}
