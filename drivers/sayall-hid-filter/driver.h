// SPDX-License-Identifier: MIT
// Adapted from QL-4/RemoteMapper, be8b57330c26a70d8b8ec9ff1e60c23251a2fc31.
#pragma once

#include <ntddk.h>
#include <wdf.h>

DRIVER_INITIALIZE DriverEntry;
EVT_WDF_DRIVER_UNLOAD SayAllEvtDriverUnload;
EVT_WDF_DRIVER_DEVICE_ADD SayAllEvtDeviceAdd;
EVT_WDF_IO_QUEUE_IO_READ SayAllEvtIoRead;
EVT_WDF_REQUEST_COMPLETION_ROUTINE SayAllReadCompletion;
