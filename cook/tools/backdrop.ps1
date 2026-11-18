Add-Type -AssemblyName System.Windows.Forms,System.Drawing
. "$PSScriptRoot\..\..\scripts\monitor.ps1"
$form = New-Object Windows.Forms.Form
$form.Text = "ROQA evaluation gray backdrop"
$form.FormBorderStyle = "None"
$form.StartPosition = "Manual"
$form.Bounds = (Get-TestScreen).Bounds
$form.BackColor = [Drawing.Color]::FromArgb(105,105,105)
$form.ShowInTaskbar = $false
$form.TopMost = $true
[Windows.Forms.Application]::Run($form)
