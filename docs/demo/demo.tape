Output docs/assets/demo.gif

Set Shell bash
Set FontSize 18
Set FontFamily "JetBrains Mono"
Set Width 1300
Set Height 520
Set Padding 24
Set Theme "Catppuccin Mocha"
Set TypingSpeed 35ms

Hide
Type "cd /vhs && python3 -m http.server 8765 --bind 127.0.0.1 -d docs/demo/mock >/dev/null 2>&1 &"
Enter
Type "export PATH=/vhs/target/release:$PATH SHOP_AMAZON_ORIGIN=http://127.0.0.1:8765 SHOP_RAKUTEN_ENDPOINT=http://127.0.0.1:8765/ichibams/api/IchibaItem/Search/20260701 RAKUTEN_APP_ID=demo RAKUTEN_ACCESS_KEY=demo SHOP_NO_UPDATE_CHECK=1"
Enter
Sleep 1s
Type "clear"
Enter
Show

Type "shop amazon search electric kettle --no-ads"
Sleep 300ms
Enter
Sleep 3.5s

Hide
Type "clear"
Enter
Show
Type "shop amazon item B0DEMOKT02 | head -16"
Sleep 300ms
Enter
Sleep 3.5s

Hide
Type "clear"
Enter
Show
Type "shop rakuten search electric kettle -n 3 --sort reviews"
Sleep 300ms
Enter
Sleep 4s
