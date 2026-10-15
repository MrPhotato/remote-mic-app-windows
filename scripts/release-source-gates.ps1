function Select-LatestPrCiRun($Response, [string]$Head, [long]$PrNumber) {
  $headRuns = @($Response.workflow_runs | Where-Object { $_.event -eq 'pull_request' -and $_.head_sha -eq $Head })
  if (!$headRuns.Count) { throw 'No Windows CI run for exact PR head' }
  if (@($headRuns | Where-Object { @($_.pull_requests | Where-Object { $null -ne $_ -and [long]$_.number -le 0 }).Count }).Count) {
    throw 'Invalid PR association metadata'
  }
  # GitHub clears pull_requests after merge. Keep such runs in the
  # latest-run selection; require checkout/tree proof below instead.
  $prRuns = @($headRuns | Where-Object { !@($_.pull_requests | Where-Object { $null -ne $_ }).Count -or @($_.pull_requests | Where-Object { $_.number -eq $PrNumber }).Count -eq 1 })
  if (!$prRuns.Count) { throw 'No Windows CI run associated with the merged PR' }
  return ($prRuns | Sort-Object -Property @{Expression={[DateTimeOffset]$_.created_at}; Descending=$true}, @{Expression={[long]$_.id}; Descending=$true}, @{Expression={[long]$_.run_attempt}; Descending=$true} | Select-Object -First 1)
}
function Assert-LatestPrCiPassed($Run, $ListedRun, [string]$Head, [long]$PrNumber) {
  if ($Run.id -ne $ListedRun.id -or $Run.event -ne 'pull_request' -or $Run.head_sha -ne $Head -or
      (@($Run.pull_requests | Where-Object { $null -ne $_ }).Count -gt 0 -and @($Run.pull_requests | Where-Object { $_.number -eq $PrNumber }).Count -ne 1) -or
      [long]$Run.run_attempt -lt 1 -or [long]$Run.run_attempt -lt [long]$ListedRun.run_attempt) {
    throw 'Cannot verify the latest CI run identity and attempt'
  }
  if ($Run.status -ne 'completed' -or $Run.conclusion -ne 'success') {
    throw 'Latest Windows CI run/attempt for the merged PR head must have completed successfully'
  }
}
function Get-TestedMergeSha([string]$Log, [long]$PrNumber) {
  $fetchPattern = '(?m)^.*\tRun actions/checkout@v4\t.*\bfetch\b.*\borigin \+(?<sha>[0-9a-f]{40}):refs/remotes/pull/' + $PrNumber + '/merge\s*$'
  $checkoutPattern = '(?m)^.*\tRun actions/checkout@v4\t.*\bcheckout --progress --force refs/remotes/pull/' + $PrNumber + '/merge\s*$'
  $commits = @([regex]::Matches($Log, $fetchPattern) | ForEach-Object { $_.Groups['sha'].Value } | Sort-Object -Unique)
  if ($commits.Count -ne 1 -or $Log -notmatch $checkoutPattern) {
    throw 'Cannot prove the selected CI attempt checked out this PR merge'
  }
  return $commits[0]
}
function Assert-TestedMergeTree($Commit, [string]$Sha, [string]$Head, [string]$SourceTree) {
  if ($Commit.sha -ne $Sha -or @($Commit.parents).Count -ne 2 -or
      @($Commit.parents | Where-Object { $_.sha -eq $Head }).Count -ne 1 -or
      $Commit.tree.sha -ne $SourceTree) {
    throw 'CI checkout commit must contain the exact PR head and release tree'
  }
}
