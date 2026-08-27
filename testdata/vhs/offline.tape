Output "testdata/vhs/output/orifude-offline.ascii"

Set Shell bash
Set Width 1200
Set Height 720
Set FontSize 18
Set TypingSpeed 20ms
Set CursorBlink false

Type "orifude"
Enter
Wait+Screen /enter.*begin/
Enter
Wait+Screen /continue/
Enter
Wait+Screen /alias/
Type "sora"
Enter
Wait+Screen /I understand/
Type "y"
Wait+Screen /The branch/

# Write and release a letter.
Enter
Wait+Screen /Write a letter/
Type "The river is quiet tonight."
Escape
Wait+Screen /Navigation mode/
Enter
Wait+Screen /Release this folded letter/
Type "y"
Wait+Screen /released/
Enter
Wait+Screen /The branch/

# Claim the incoming fixture and reply.
Type "j"
Enter
Wait+Screen /Folded delivery/
Enter
Wait+Screen /Fold a reply/
Enter
Wait+Screen /Write one reply/
Type "I heard it too."
Escape
Wait+Screen /Navigation mode/
Enter
Wait+Screen /Release this one reply/
Type "y"
Wait+Screen /Your one reply/
Enter
Wait+Screen /The branch/

# Reopen a keepsake, then exercise the settings form.
Type "jj"
Enter
Wait+Screen /Keepsakes/
Enter
Wait+Screen /From mori/
Type "b"
Wait+Screen /Keepsakes/
Type "b"
Wait+Screen /The branch/
Type "G"
Enter
Wait+Screen /Connection: offline prototype/
Enter
Wait+Screen /Theme and contrast/
Enter
Right
Enter
Right
Enter
Enter
Wait+Screen /Settings applied/

# Keep a draft when declining the quit confirmation.
Type "b"
Wait+Screen /The branch/
Type "g"
Type "g"
Enter
Type "unfinished thought"
Escape
Wait+Screen /Navigation mode/
Type "q"
Wait+Screen /Discard the current draft/
Type "n"
Wait+Screen /unfinished thought/
Type "q"
Type "y"
