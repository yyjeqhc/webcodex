$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationFramework
$window = New-Object Windows.Window
$window.Title = 'WebCodex bounded scroll fixture'
$window.Width = 760
$window.Height = 540
$dock = New-Object Windows.Controls.DockPanel
$window.Content = $dock
$status = New-Object Windows.Controls.TextBlock
[Windows.Controls.DockPanel]::SetDock($status, 'Top')
[void]$dock.Children.Add($status)
$outer = New-Object Windows.Controls.ScrollViewer
$outer.HorizontalScrollBarVisibility = 'Auto'
$outer.VerticalScrollBarVisibility = 'Auto'
[Windows.Automation.AutomationProperties]::SetName($outer, 'Scroll outer container')
$canvas = New-Object Windows.Controls.Canvas
$canvas.Width = 1800
$canvas.Height = 2000
$outer.Content = $canvas
[void]$dock.Children.Add($outer)
$inner = New-Object Windows.Controls.ScrollViewer
$inner.Width = 300
$inner.Height = 150
$inner.HorizontalScrollBarVisibility = 'Disabled'
$inner.VerticalScrollBarVisibility = 'Auto'
[Windows.Automation.AutomationProperties]::SetName($inner, 'Scroll inner container')
[Windows.Controls.Canvas]::SetLeft($inner, 30)
[Windows.Controls.Canvas]::SetTop($inner, 50)
$rows = New-Object Windows.Controls.StackPanel
foreach ($index in 1..40) {
    $row = New-Object Windows.Controls.TextBlock
    $row.Text = "Scroll row $index"
    $row.Height = 28
    [void]$rows.Children.Add($row)
}
$inner.Content = $rows
[void]$canvas.Children.Add($inner)
$disabled = New-Object Windows.Controls.ScrollViewer
$disabled.Width = 100
$disabled.Height = 100
$disabled.IsEnabled = $false
[Windows.Automation.AutomationProperties]::SetName($disabled, 'Scroll disabled container')
[Windows.Controls.Canvas]::SetLeft($disabled, 400)
[void]$canvas.Children.Add($disabled)
$label = New-Object Windows.Controls.TextBlock
$label.Text = 'Scroll unsupported label'
[Windows.Controls.Canvas]::SetTop($label, 240)
[void]$canvas.Children.Add($label)
$bottom = New-Object Windows.Controls.TextBlock
$bottom.Text = 'Scroll bottom business target'
[Windows.Controls.Canvas]::SetTop($bottom, 1850)
[void]$canvas.Children.Add($bottom)
function Update-ScrollStatus {
    $status.Text = 'scroll-business:outer={0},{1};inner={2}' -f $outer.HorizontalOffset, $outer.VerticalOffset, $inner.VerticalOffset
    if ($env:WEBCODEX_SCROLL_FIXTURE_PROBE) {
        $probe = [ordered]@{outer_x=$outer.HorizontalOffset;outer_y=$outer.VerticalOffset;inner_y=$inner.VerticalOffset;pid=$PID}
        $probe | ConvertTo-Json -Compress | Set-Content -LiteralPath $env:WEBCODEX_SCROLL_FIXTURE_PROBE -Encoding utf8
    }
}
$outer.Add_ScrollChanged({ Update-ScrollStatus })
$inner.Add_ScrollChanged({ Update-ScrollStatus })
$window.Add_ContentRendered({ Update-ScrollStatus })
$timer = New-Object Windows.Threading.DispatcherTimer
$timer.Interval = [TimeSpan]::FromSeconds(120)
$timer.Add_Tick({ $window.Close() })
$timer.Start()
[void]$window.ShowDialog()
