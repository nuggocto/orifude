Output "testdata/vhs/output/orifude-online.ascii"

Set Shell bash
Set Width 1200
Set Height 720
Set FontSize 18
Set TypingSpeed 20ms
Set CursorBlink false

Type "orifude"
Enter
Wait+Screen /Create an identity/
Enter
Wait+Screen /A quiet post office/
Enter
Wait+Screen /invite code/
Type "R0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0dHR0c"
Enter
Wait+Screen /private alias/
Type "sora"
Enter
Wait+Screen /I understand/
Type "y"
Wait+Screen /owner-only local file/
Type "y"
Wait+Screen /I saved it/
Enter
Wait+Screen /I stored it safely/
Type "y"
Wait+Screen /Welcome to the branch, sora/

# Release a letter through the production HTTP handler.
Enter
Wait+Screen /Write a letter/
Type "The river is quiet tonight."
Escape
Wait+Screen /Navigation mode/
Enter
Wait+Screen /Release this folded letter/
Type "y"
Wait+Screen /The letter was released/
Enter
Wait+Screen /The branch/
Type "q"
