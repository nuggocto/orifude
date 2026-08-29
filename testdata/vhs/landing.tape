Output "testdata/vhs/output/orifude-landing.webm"
Output "testdata/vhs/output/orifude-landing.ascii"

Set Shell bash
Set Width 1200
Set Height 720
Set FontSize 18
Set TypingSpeed 18ms
Set CursorBlink false
Set Framerate 30

Type "ORIFUDE_OFFLINE_DEMO=1 orifude"
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
Sleep 1s

# Write, fold, and release a letter.
Enter
Wait+Screen /Write a letter/
Type "The river is quiet tonight."
Escape
Wait+Screen /Navigation mode/
Enter
Wait+Screen /Release this folded letter/
Type "y"
Wait+Screen /released/
Sleep 1s
Enter
Wait+Screen /The branch/

# Claim and explicitly unfold a waiting letter.
Type "j"
Enter
Wait+Screen /Folded delivery/
Sleep 1s
Enter
Wait+Screen /Fold a reply/
Sleep 2s

# Send the single allowed reply.
Enter
Wait+Screen /Write one reply/
Type "I heard it too."
Escape
Wait+Screen /Navigation mode/
Enter
Wait+Screen /Release this one reply/
Type "y"
Wait+Screen /Your one reply/
Sleep 2s
Enter
Wait+Screen /The branch/
Type "q"
