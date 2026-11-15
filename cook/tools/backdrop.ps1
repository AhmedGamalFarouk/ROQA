Add-Type -AssemblyName System.Windows.Forms,System.Drawing
$form = New-Object Windows.Forms.Form
$form.Text = "ROQA evaluation gray backdrop"
$form.FormBorderStyle = "None"
$form.StartPosition = "Manual"
$form.Bounds = New-Object Drawing.Rectangle(0,0,1920,1080)
$form.BackColor = [Drawing.Color]::FromArgb(105,105,105)
$form.ShowInTaskbar = $false
$form.TopMost = $true
[Windows.Forms.Application]::Run($form)
