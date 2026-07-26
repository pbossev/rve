# VHS documentation
#
# Output:
#   Output <path>.gif               Create a GIF output at the given <path>
#   Output <path>.mp4               Create an MP4 output at the given <path>
#   Output <path>.webm              Create a WebM output at the given <path>
#
# Require:
#   Require <string>                Ensure a program is on the $PATH to proceed
#
# Settings:
#   Set FontSize <number>           Set the font size of the terminal
#   Set FontFamily <string>         Set the font family of the terminal
#   Set Height <number>             Set the height of the terminal
#   Set Width <number>              Set the width of the terminal
#   Set LetterSpacing <float>       Set the font letter spacing (tracking)
#   Set LineHeight <float>          Set the font line height
#   Set LoopOffset <float>%         Set the starting frame offset for the GIF loop
#   Set Theme <json|string>         Set the theme of the terminal
#   Set Padding <number>            Set the padding of the terminal
#   Set Framerate <number>          Set the framerate of the recording
#   Set PlaybackSpeed <float>       Set the playback speed of the recording
#   Set MarginFill <file|#000000>   Set the file or color the margin will be filled with.
#   Set Margin <number>             Set the size of the margin. Has no effect if MarginFill isn't set.
#   Set BorderRadius <number>       Set terminal border radius, in pixels.
#   Set WindowBar <string>          Set window bar type. (one of: Rings, RingsRight, Colorful, ColorfulRight)
#   Set WindowBarSize <number>      Set window bar size, in pixels. Default is 40.
#   Set TypingSpeed <time>          Set the typing speed of the terminal. Default is 50ms.
#
# Sleep:
#   Sleep <time>                    Sleep for a set amount of <time> in seconds
#
# Type:
#   Type[@<time>] "<characters>"    Type <characters> into the terminal with a
#                                   <time> delay between each character
#
# Keys:
#   Escape[@<time>] [number]        Press the Escape key
#   Backspace[@<time>] [number]     Press the Backspace key
#   Delete[@<time>] [number]        Press the Delete key
#   Insert[@<time>] [number]        Press the Insert key
#   Down[@<time>] [number]          Press the Down key
#   Enter[@<time>] [number]         Press the Enter key
#   Space[@<time>] [number]         Press the Space key
#   Tab[@<time>] [number]           Press the Tab key
#   Left[@<time>] [number]          Press the Left Arrow key
#   Right[@<time>] [number]         Press the Right Arrow key
#   Up[@<time>] [number]            Press the Up Arrow key
#   Down[@<time>] [number]          Press the Down Arrow key
#   PageUp[@<time>] [number]        Press the Page Up key
#   PageDown[@<time>] [number]      Press the Page Down key
#   Ctrl+<key>                      Press the Control key + <key> (e.g. Ctrl+C)
#
# Display:
#   Hide                            Hide the subsequent commands from the output
#   Show                            Show the subsequent commands in the output

Output demo/out/demo.gif
Output demo/out/demo.webm
Set Theme "Dark+"
Set FontSize 12
Set Width 1200
Set Height 600
Set Framerate 20
Set PlaybackSpeed 1.0

Type "./rve demo/test_video.mp4"
Enter

Sleep 1s
Screenshot demo/out/screenshot_1.png

# Play video for 1.5 seconds, then pause
Type " "
Sleep 1.5s
Screenshot demo/out/screenshot_2.png
Sleep 1s
Screenshot demo/out/screenshot_3.png

# Jump to 30% and place a marker
Sleep 500ms
Type "3"
Sleep 500ms
Screenshot demo/out/screenshot_4.png
Type "v"
Sleep 500ms
Screenshot demo/out/screenshot_5.png

# Jump to 70% and place a marker
Sleep 500ms
Type "7"
Sleep 500ms
Screenshot demo/out/screenshot_6.png
Type "v"
Sleep 500ms
Screenshot demo/out/screenshot_7.png

# Jump to previous marker and toggle segment inclusion
Sleep 500ms
Type "["
Sleep 500ms
Screenshot demo/out/screenshot_8.png
Type "t"
Sleep 500ms
Screenshot demo/out/screenshot_9.png
Sleep 1s

# Save output
Type "s"
Sleep 1.5s
Screenshot demo/out/screenshot_10.png
Enter
Sleep 500ms

# Quit rve
Type "q"
Sleep 500ms
Type "y"
Sleep 1s