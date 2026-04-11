Output site/demo-claude.gif

Set Shell "bash"
Set FontSize 13
Set Width 640
Set Height 120
Set Padding 24
Set Framerate 24
Set PlaybackSpeed 0.85

Set Theme {
  "name": "shipit",
  "black":         "#0d1117",
  "red":           "#ff7b72",
  "green":         "#3fb950",
  "yellow":        "#d29922",
  "blue":          "#7eb8f7",
  "magenta":       "#bc8cff",
  "cyan":          "#00c8ff",
  "white":         "#cdd9e5",
  "brightBlack":   "#6e7681",
  "brightRed":     "#ff7b72",
  "brightGreen":   "#56d364",
  "brightYellow":  "#e3b341",
  "brightBlue":    "#7eb8f7",
  "brightMagenta": "#d2a8ff",
  "brightCyan":    "#00c8ff",
  "brightWhite":   "#e6edf3",
  "background":    "#0d1117",
  "foreground":    "#cdd9e5",
  "selection":     "#264f78",
  "cursor":        "#00c8ff"
}

Hide
  Type "source site/mock-shipit.sh"
  Enter
  Sleep 100ms
  Type "clear"
  Enter
  Sleep 100ms

Show

Sleep 1s

Type "shipit claude"
Sleep 500ms
Enter
Sleep 800ms

Sleep 2s
