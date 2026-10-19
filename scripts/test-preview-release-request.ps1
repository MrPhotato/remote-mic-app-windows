$ErrorActionPreference='Stop'
$requestScript=Join-Path $PSScriptRoot 'request-preview-release.ps1'
$global:sayallReleaseRequestTest=@{source=('a'*40);calls=@();fixture=$null;failedLabel=$false}
function gh {
    $global:sayallReleaseRequestTest.calls += ,@($args)
    $global:LASTEXITCODE=0
    if($args[0] -eq 'api' -and $args[1] -eq 'repos/MrPhotato/remote-mic-app-windows/pulls/12') {
        return $global:sayallReleaseRequestTest.fixture | ConvertTo-Json -Depth 8
    }
    if($args[0] -eq 'api' -and $args[1] -eq 'repos/MrPhotato/remote-mic-app-windows/issues/12/labels') {
        if($global:sayallReleaseRequestTest.failedLabel){$global:LASTEXITCODE=1;return}
        if($args[2] -ne '--method' -or $args[3] -ne 'POST' -or $args[4] -ne '-f' -or $args[5] -ne 'labels[]=release:preview'){throw 'Label operation must append with REST POST'}
        $global:sayallReleaseRequestTest.fixture.labels += @{name='release:preview'}
        return '[]'
    }
    if(($args -join ' ') -eq "workflow run preview-cd.yml --repo MrPhotato/remote-mic-app-windows --ref main -f pull_request=12 -f expected_head=$($global:sayallReleaseRequestTest.source)") {return}
    throw 'Unexpected GitHub request; GraphQL and permission expansion are forbidden'
}
try {
    foreach($case in @('valid','wrong_head','draft','wrong_repo','label_failure')) {
        $global:sayallReleaseRequestTest.calls=@()
        $global:sayallReleaseRequestTest.fixture=@{state='open';draft=$false;base=@{ref='main'};head=@{sha=$global:sayallReleaseRequestTest.source;repo=@{full_name='MrPhotato/remote-mic-app-windows'}};labels=@(@{name='existing-label'});html_url='https://github.com/MrPhotato/remote-mic-app-windows/pull/12'}
        $global:sayallReleaseRequestTest.failedLabel=$case -eq 'label_failure'
        if($case -eq 'wrong_head'){$global:sayallReleaseRequestTest.fixture.head.sha='b'*40}
        if($case -eq 'draft'){$global:sayallReleaseRequestTest.fixture.draft=$true}
        if($case -eq 'wrong_repo'){$global:sayallReleaseRequestTest.fixture.head.repo.full_name='other/repo'}
        $failed=$false
        $reason=''
        try {& $requestScript -PullRequest 12 -ExpectedHead $global:sayallReleaseRequestTest.source | Out-Null}catch{$failed=$true;$reason=$_.Exception.Message}
        if($case -eq 'valid') {
            if($failed -or $global:sayallReleaseRequestTest.calls.Count -ne 4 -or 'existing-label' -notin $global:sayallReleaseRequestTest.fixture.labels.name){throw "Valid request failed or existing label changed: $reason; calls=$($global:sayallReleaseRequestTest.calls.Count)"}
        } else {
            if(!$failed -or @($global:sayallReleaseRequestTest.calls|Where-Object {$_[0] -eq 'workflow'}).Count){throw 'Invalid request dispatched'}
        }
    }
}finally{Remove-Item Function:gh;Remove-Variable sayallReleaseRequestTest -Scope Global}
'Preview request fixtures: 5 passed'
