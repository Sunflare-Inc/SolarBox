-- SolarBox bounce: a ball that bounces around. Z flips its colour.
bx, by, dx, dy, col = 64, 30, 1.3, 0.9, 11

function _update60()
  bx = bx + dx
  by = by + dy
  if bx < 4 or bx > 123 then dx = -dx end
  if by < 4 or by > 123 then dy = -dy end
  if btnp(4) then col = 8 + flr(rnd(8)) end
end

function _draw()
  cls(1)
  circfill(bx, by, 4, col)
  print("BOUNCE", 52, 60, 13)
end
