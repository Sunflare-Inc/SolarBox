-- SolarBox palette viewer: shows all 16 colours with their numbers.
function _draw()
  cls(0)
  print("PALETTE", 48, 4, 7)
  for i = 0, 15 do
    local x = (i % 4) * 30 + 6
    local y = flr(i / 4) * 26 + 16
    rectfill(x, y, x + 24, y + 16, i)
    rect(x, y, x + 24, y + 16, 5)
    print(i, x + 2, y + 19, 6)
  end
end
