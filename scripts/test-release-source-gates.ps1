$ErrorActionPreference = 'Stop'
$workflow = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../.github/workflows/windows-release.yml') -Raw
$match = [regex]::Match($workflow, '(?ms)^          function Select-LatestPrCiRun.*?(?=^          \$runs = )')
if (!$match.Success) { throw 'Cannot locate production gate functions' }
$source = $match.Value -replace '(?m)^ {10}', ''
$tokens = $null; $errors = $null
$gateAst = [Management.Automation.Language.Parser]::ParseInput($source, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw 'Gate PowerShell parse failed' }
foreach ($name in @('Select-LatestPrCiRun','Assert-LatestPrCiPassed','Get-TestedMergeSha','Assert-TestedMergeTree')) {
    $function = $gateAst.Find({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name}, $true)
    if (!$function) { throw 'Gate function missing' }
    . ([scriptblock]::Create($function.Extent.Text))
}
function New-FixtureRun([long]$Id, [string]$Conclusion = 'success', [string]$Status = 'completed', [int]$Attempt = 1, [object]$PrNumbers = @(7), [string]$Head = ('a' * 40)) {
    [pscustomobject]@{
        id = $Id; head_sha = $Head; event = 'pull_request'; status = $Status
        conclusion = $Conclusion; run_attempt = $Attempt
        created_at = [DateTimeOffset]::FromUnixTimeSeconds(1800000000 + $Id).ToString('o')
        pull_requests = @($PrNumbers | ForEach-Object { [pscustomobject]@{number=$_} })
    }
}
$cases = @(
    @{name='latest_success_after_failure'; runs=@((New-FixtureRun 10 'failure'),(New-FixtureRun 11)); accept=$true; selected=11},
    @{name='old_success_new_failure'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 'failure')); accept=$false},
    @{name='old_success_new_cancelled'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 'cancelled')); accept=$false},
    @{name='old_success_new_queued'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 '' 'queued')); accept=$false},
    @{name='old_success_new_running'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 '' 'in_progress')); accept=$false},
    @{name='old_success_new_no_conclusion'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 '')); accept=$false},
    @{name='only_other_pr'; runs=@((New-FixtureRun 11 -PrNumbers @(8))); accept=$false},
    @{name='other_pr_does_not_replace_current_pr'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 'failure' -PrNumbers @(8))); accept=$true; selected=10},
    @{name='old_success_new_empty_pr'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 -PrNumbers @())); accept=$false},
    @{name='old_success_new_null_pr'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 -PrNumbers $null)); accept=$false},
    @{name='old_success_new_invalid_pr'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 -PrNumbers @(0))); accept=$false},
    @{name='other_head_only'; runs=@((New-FixtureRun 11 -Head ('b' * 40))); accept=$false},
    @{name='new_attempt_failed'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 'failure' -Attempt 2); accept=$false},
    @{name='new_attempt_succeeded'; runs=@((New-FixtureRun 11 'failure')); detail=(New-FixtureRun 11 -Attempt 2); accept=$true; selected=11},
    @{name='new_attempt_queued'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 '' 'queued' -Attempt 2); accept=$false},
    @{name='detail_wrong_pr'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 -PrNumbers @(8)); accept=$false},
    @{name='detail_empty_pr'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 -PrNumbers @()); accept=$false},
    @{name='detail_wrong_head'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 -Head ('b' * 40)); accept=$false},
    @{name='detail_wrong_run'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 12); accept=$false},
    @{name='detail_no_attempt'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 -Attempt 0); accept=$false},
    @{name='detail_attempt_regression'; runs=@((New-FixtureRun 11 -Attempt 2)); detail=(New-FixtureRun 11 -Attempt 1); accept=$false},
    @{name='latest_attempt_wins_for_same_run'; runs=@((New-FixtureRun 11),(New-FixtureRun 11 'failure' -Attempt 2)); accept=$false},
    @{name='no_runs'; runs=@(); accept=$false},
    @{name='merged_empty_association_with_proof'; runs=@((New-FixtureRun 11 -PrNumbers @())); proof=$true; accept=$true; selected=11},
    @{name='new_empty_failed_with_proof'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 'failure' -PrNumbers @())); proof=$true; accept=$false},
    @{name='new_empty_running_with_proof'; runs=@((New-FixtureRun 10),(New-FixtureRun 11 '' 'in_progress' -PrNumbers @())); proof=$true; accept=$false},
    @{name='detail_empty_with_proof'; runs=@((New-FixtureRun 11)); detail=(New-FixtureRun 11 -PrNumbers @()); proof=$true; accept=$true; selected=11}
)
$testedSha = 'b' * 40
$tree = 'c' * 40
$checkoutLog = "verify`tRun actions/checkout@v4`t2026-09-20T00:00:00Z git fetch --depth=1 origin +${testedSha}:refs/remotes/pull/7/merge`nverify`tRun actions/checkout@v4`t2026-09-20T00:00:01Z git checkout --progress --force refs/remotes/pull/7/merge"
$commit = @{sha=$testedSha; tree=@{sha=$tree}; parents=@(@{sha=('d'*40)},@{sha=('a'*40)})} | ConvertTo-Json -Depth 5 | ConvertFrom-Json
$passed = 0
foreach ($case in $cases) {
    # Exercise the exact workflow functions against JSON-shaped API fixtures.
    $response = @{workflow_runs=$case.runs} | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $accepted = $false
    $selectedId = $null
    try {
        $latest = Select-LatestPrCiRun $response ('a' * 40) 7
        $selectedId = $latest.id
        $detail = if ($case.ContainsKey('detail')) { $case.detail | ConvertTo-Json -Depth 8 | ConvertFrom-Json } else { $latest }
        Assert-LatestPrCiPassed $detail $latest ('a' * 40) 7
        if (!@($latest.pull_requests | Where-Object { $null -ne $_ }).Count -or !@($detail.pull_requests | Where-Object { $null -ne $_ }).Count) {
            if (!$case.ContainsKey('proof')) { throw 'Checkout proof unavailable' }
            $proofSha = Get-TestedMergeSha $checkoutLog 7
            Assert-TestedMergeTree $commit $proofSha ('a'*40) $tree
        }
        $accepted = $true
    } catch { }
    if ($accepted -ne $case.accept -or ($accepted -and $selectedId -ne $case.selected)) { throw "Fixture failed: $($case.name)" }
    $passed++
}

$proofCases = @(
    @{name='valid_checkout_and_tree'; log=$checkoutLog; commit=$commit; accept=$true},
    @{name='wrong_pr'; log=$checkoutLog.Replace('pull/7/', 'pull/8/'); commit=$commit; accept=$false},
    @{name='wrong_step'; log=$checkoutLog.Replace('Run actions/checkout@v4', 'Build application'); commit=$commit; accept=$false},
    @{name='no_fetch'; log=($checkoutLog -split "`n")[1]; commit=$commit; accept=$false},
    @{name='no_checkout'; log=($checkoutLog -split "`n")[0]; commit=$commit; accept=$false},
    @{name='ambiguous_commit'; log=($checkoutLog+"`n"+$checkoutLog.Replace($testedSha, ('e'*40))); commit=$commit; accept=$false},
    @{name='tree_mismatch'; log=$checkoutLog; commit=@{sha=$testedSha;tree=@{sha=('e'*40)};parents=$commit.parents}; accept=$false},
    @{name='head_mismatch'; log=$checkoutLog; commit=@{sha=$testedSha;tree=@{sha=$tree};parents=@(@{sha=('e'*40)},@{sha=('d'*40)})}; accept=$false},
    @{name='not_merge_commit'; log=$checkoutLog; commit=@{sha=$testedSha;tree=@{sha=$tree};parents=@(@{sha=('a'*40)})}; accept=$false},
    @{name='commit_identity_mismatch'; log=$checkoutLog; commit=@{sha=('e'*40);tree=@{sha=$tree};parents=$commit.parents}; accept=$false}
)
foreach ($case in $proofCases) {
    $accepted=$false
    try {
        $sha=Get-TestedMergeSha $case.log 7
        Assert-TestedMergeTree $case.commit $sha ('a'*40) $tree
        $accepted=$true
    } catch { }
    if ($accepted -ne $case.accept) { throw "Checkout proof fixture failed: $($case.name)" }
    $passed++
}
[pscustomobject]@{check='release_source_gate_fixtures';passed=$passed;failed=0;remote_queries=0;workflow_function_source='actual_yaml_functions'}|ConvertTo-Json -Compress
