// SPDX-License-Identifier: MIT
#include <stdio.h>
#include <string.h>
#include "../remap.h"

static unsigned int checks;
static unsigned int failures;

static void Check(int condition, const char *name)
{
    checks++;
    if (!condition) {
        printf("FAIL %s\n", name);
        failures++;
    }
}

static void CheckReport(UCHAR *report, SIZE_T length, UCHAR expectedUsage,
    BOOLEAN changed, const char *name)
{
    UCHAR expected[121];
    memcpy(expected, report, sizeof(expected));
    expected[3] = expectedUsage;
    Check(SayAllRemapReport(report, length) == changed, name);
    // Includes bytes beyond length: the converter must never write them.
    Check(memcmp(report, expected, sizeof(expected)) == 0, name);
}

int main(void)
{
    UCHAR report[121];
    UCHAR before[121];
    const UCHAR source[] = { 0x80, 0x81, 0xF1 };
    const UCHAR replacement[] = { 0x68, 0x69, 0x6A };
    const UCHAR lengths[] = { 4, 7, 9, 121 };
    size_t i;
    size_t j;
    unsigned int usage;

    Check(!SayAllRemapReport(NULL, 0), "null empty");
    Check(!SayAllRemapReport(NULL, 121), "null nonempty");

    for (i = 0; i < sizeof(source); i++) {
        for (j = 0; j < sizeof(lengths); j++) {
            memset(report, 0xA5, sizeof(report));
            report[0] = 1;
            report[1] = 0;
            report[2] = 0;
            report[3] = source[i];
            CheckReport(report, lengths[j], replacement[i], TRUE, "three usages with bounded lengths");
            CheckReport(report, lengths[j], replacement[i], FALSE, "idempotent already remapped report");
        }
    }

    for (i = 0; i < 4; i++) {
        memset(report, 0, sizeof(report));
        report[0] = 1;
        report[3] = 0x80;
        CheckReport(report, i, 0x80, FALSE, "short report unchanged");
    }

    // Exhaustive source-byte check protects F5, navigation, power, and key-up.
    for (usage = 0; usage <= 0xFF; usage++) {
        UCHAR expected = (UCHAR)usage;
        BOOLEAN changed = FALSE;
        for (i = 0; i < sizeof(source); i++) {
            if (usage == source[i]) {
                expected = replacement[i];
                changed = TRUE;
            }
        }
        memset(report, 0, sizeof(report));
        report[0] = 1;
        report[3] = (UCHAR)usage;
        CheckReport(report, sizeof(report), expected, changed, "only three source usages change");
    }

    // All other report IDs, including vendor reports 6/7/8, pass untouched.
    for (usage = 0; usage <= 0xFF; usage++) {
        if (usage == 1) continue;
        memset(report, 0x80, sizeof(report));
        report[0] = (UCHAR)usage;
        memcpy(before, report, sizeof(report));
        Check(!SayAllRemapReport(report, sizeof(report)), "non-keyboard report untouched");
        Check(memcmp(report, before, sizeof(report)) == 0, "vendor payload unchanged");
    }

    memset(report, 0, sizeof(report));
    report[0] = 1;
    report[1] = 0x02;
    report[2] = 0x7F;
    report[3] = 0x80;
    report[4] = 0x81;
    report[5] = 0xF1;
    report[6] = 0x3E;
    CheckReport(report, sizeof(report), 0x68, TRUE, "only first slot changes; modifiers preserved");
    report[3] = 0;
    CheckReport(report, sizeof(report), 0, FALSE, "release with other slots is not synthesised");

    // Repeated presses, key switches, release, and a fresh caller require no
    // remembered state. A process exit cannot leave a held key in this filter.
    for (i = 0; i < sizeof(source); i++) {
        memset(report, 0, sizeof(report));
        report[0] = 1;
        report[3] = source[i];
        CheckReport(report, sizeof(report), replacement[i], TRUE, "press");
        report[3] = source[i];
        CheckReport(report, sizeof(report), replacement[i], TRUE, "repeat press");
        report[3] = 0;
        CheckReport(report, sizeof(report), 0, FALSE, "release after remapped press");
    }

    printf("%s: %u checks, %u failures\n", failures == 0 ? "PASS" : "FAIL", checks, failures);
    return failures == 0 ? 0 : 1;
}
