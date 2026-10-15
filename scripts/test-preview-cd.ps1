$ErrorActionPreference='Stop'
$controller=Join-Path $PSScriptRoot 'continue-preview-cd.ps1'
$fixtureDirectory=Join-Path $PSScriptRoot ('../target/preview-cd-tests/'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixtureDirectory -Force | Out-Null
$saved=@{}
foreach($key in @('GH_TOKEN','GITHUB_REPOSITORY','GITHUB_EVENT_NAME','GITHUB_EVENT_PATH','GITHUB_REF','GITHUB_STEP_SUMMARY','REQUESTED_PR','EXPECTED_HEAD')) {
    $saved[$key]=[Environment]::GetEnvironmentVariable($key)
}
function Invoke-RestMethod {
    param([string]$Uri,$Headers)
    $f=$global:sayallPreviewFixture
    if($Uri -match '/pulls/7$') {
        $f.prReads++
        if($f.cancel -and $f.prReads -ge 2){$f.pr.labels=@()}
        return $f.pr
    }
    if($Uri -match '/runs\?') {return @{workflow_runs=@($f.run)}}
    if($Uri -match '/actions/runs/10$') {
        $f.runReads++
        if($f.rerun -and $f.runReads -ge 2){$f.run.run_attempt=2}
        return ($f.run | ConvertTo-Json -Depth 10 | ConvertFrom-Json)
    }
    if($Uri -like '*/git/ref/heads/main') {return @{object=@{sha=$f.main}}}
    if($Uri -like '*/git/ref/tags/*') {return $f.tag}
    if($Uri -match '/git/commits/([a-f0-9]{40})$') {
        return @{sha=$Matches[1];tree=@{sha=('c'*40)};parents=@(@{sha=('a'*40)},@{sha=('b'*40)})}
    }
    if($Uri -match '/contents/src-tauri/tauri.conf.json') {
        return @{content=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes('{"version":"1.2.3"}'))}
    }
    if($Uri -match '/contents/releases/v1.2.3.md') {if($f.noNotes){throw 'Missing notes'};return @{content='notes'}}
    if($Uri -match '/commits/[a-f0-9]+/pulls$') {return @($f.pr)}
    throw "Unexpected fixture API: $Uri"
}
function gh {
    $global:LASTEXITCODE=0
    $f=$global:sayallPreviewFixture
    if($args[0] -eq 'run') {
        return "verify`tRun actions/checkout@v4`tgit fetch origin +$('e'*40):refs/remotes/pull/7/merge`nverify`tRun actions/checkout@v4`tgit checkout --progress --force refs/remotes/pull/7/merge"
    }
    if($args[0] -eq 'api') {
        if('sha=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' -notin $args -or 'merge_method=merge' -notin $args){throw 'Unsafe merge arguments'}
        $f.merges++;$f.pr.merged=$true;$f.pr.state='closed';$f.pr.merge_commit_sha='d'*40
        $f.main=if($f.movedAfterMerge){'f'*40}else{'d'*40}
        return
    }
    if($args[0] -eq 'workflow') {
        if('windows-release.yml' -notin $args -or 'tag=v1.2.3' -notin $args -or 'main' -notin $args){throw 'Wrong release dispatch'}
        $f.dispatches++;if($f.dispatchFailure){$global:LASTEXITCODE=1};return
    }
    throw 'Unexpected gh operation'
}
$cases=@(
    @{name='authorized successful merge and dispatch';ok=$true;merges=1;dispatch=1},
    @{name='ordinary PR no-op';ok=$true;change={param($f)$f.pr.labels=@()}},
    @{name='queued CI waits for completion event';ok=$true;change={param($f)$f.run.status='queued';$f.run.conclusion=$null}},
    @{name='failed CI';ok=$false;change={param($f)$f.run.conclusion='failure'}},
    @{name='head changed';ok=$false;change={param($f)$f.pr.head.sha='f'*40}},
    @{name='fork PR';ok=$false;change={param($f)$f.pr.head.repo.full_name='other/repo'}},
    @{name='draft PR';ok=$false;change={param($f)$f.pr.draft=$true}},
    @{name='stale main parent';ok=$false;change={param($f)$f.main='f'*40}},
    @{name='missing notes';ok=$false;change={param($f)$f.noNotes=$true}},
    @{name='existing version';ok=$false;change={param($f)$f.tag=@{object=@{type='commit';sha=('f'*40)}}}},
    @{name='CI rerun while verifying';ok=$false;change={param($f)$f.rerun=$true}},
    @{name='label removed while verifying';ok=$false;change={param($f)$f.cancel=$true}},
    @{name='main moved after merge blocks release';ok=$false;merges=1;change={param($f)$f.movedAfterMerge=$true}},
    @{name='dispatch failure retains merged PR';ok=$false;merges=1;dispatch=1;change={param($f)$f.dispatchFailure=$true}},
    @{name='merged PR retry dispatches without second merge';ok=$true;dispatch=1;change={param($f)$f.pr.merged=$true;$f.pr.state='closed';$f.pr.merge_commit_sha='d'*40;$f.main='d'*40}},
    @{name='workflow_run event takes same verified path';ok=$true;merges=1;dispatch=1;event=$true},
    @{name='workflow_run empty association uses commit lookup';ok=$true;merges=1;dispatch=1;event=$true;emptyAssociation=$true}
)
$passed=0
try {
    $env:GH_TOKEN='non-secret-fixture';$env:GITHUB_REPOSITORY='example/sayall';$env:GITHUB_REF='refs/heads/main'
    $env:REQUESTED_PR='7';$env:EXPECTED_HEAD='a'*40;$env:GITHUB_STEP_SUMMARY=Join-Path $fixtureDirectory 'summary.txt'
    $env:GITHUB_EVENT_PATH=Join-Path $fixtureDirectory 'event.json'
    foreach($case in $cases) {
        $global:sayallPreviewFixture=@{
            pr=@{number=7;state='open';merged=$false;draft=$false;head=@{sha=('a'*40);repo=@{full_name='example/sayall'}};base=@{ref='main'};labels=@(@{name='release:preview'})}
            run=@{id=10;head_sha=('a'*40);event='pull_request';status='completed';conclusion='success';run_attempt=1;created_at='2026-10-15T00:00:00Z';pull_requests=@(@{number=7})}
            main=('b'*40);tag=$null;prReads=0;runReads=0;merges=0;dispatches=0
        }
        if($case.change){& $case.change $global:sayallPreviewFixture}
        $env:GITHUB_EVENT_NAME=if($case.event){'workflow_run'}else{'workflow_dispatch'}
        $eventRun=$global:sayallPreviewFixture.run.Clone()
        $eventRun.head_repository=@{full_name='example/sayall'}
        $eventRun.pull_requests=if($case.emptyAssociation){@()}else{@(@{number=7;base=@{ref='main'}})}
        @{workflow_run=$eventRun}|ConvertTo-Json -Depth 10|Set-Content $env:GITHUB_EVENT_PATH
        $accepted=$false;$reason=''
        try{& $controller;$accepted=$true}catch{$reason=$_.Exception.Message}
        if($accepted -ne $case.ok -or $global:sayallPreviewFixture.merges -ne [int]$case.merges -or $global:sayallPreviewFixture.dispatches -ne [int]$case.dispatch){
            throw "Preview CD fixture failed: $($case.name); accepted=$accepted; $reason"
        }
        $passed++
    }
} finally {
    foreach($key in $saved.Keys){[Environment]::SetEnvironmentVariable($key,$saved[$key])}
    Remove-Item function:gh,function:Invoke-RestMethod
    Remove-Variable sayallPreviewFixture -Scope Global
}
Write-Host "Preview CD orchestration: $passed passed"
