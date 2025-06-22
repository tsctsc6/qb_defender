$ErrorActionPreference = "Stop";
$response = Invoke-WebRequest -Uri "http://127.0.0.1:port/api/v2/app/preferences" -Method GET;
$jsonData = $response.Content | ConvertFrom-Json;
$jsonData.banned_IPs;
$count = $jsonData.banned_IPs.Split("`n").Length;
Write-Host $count "ips are banned!";
pause;
