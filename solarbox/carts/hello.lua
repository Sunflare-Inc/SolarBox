-- SolarBox demo cart: arrows move the sun, Z changes its colour.
x, y, col = 64, 64, 10

function _init()
  stars = {}
  for i = 1, 40 do
    add(stars, {x = rnd(128), y = rnd(128), s = 0.1 + rnd(0.5)})
  end
end

function _update60()
  if btn(0) then x = x - 1 end
  if btn(1) then x = x + 1 end
  if btn(2) then y = y - 1 end
  if btn(3) then y = y + 1 end
  if btnp(4) then col = 8 + flr(rnd(8)) end
  foreach(stars, function(s)
    s.x = (s.x - s.s) % 128
  end)
end

function _draw()
  cls(1)
  foreach(stars, function(s) pset(s.x, s.y, 6) end)
  circfill(x, y, 8, col)
  circ(x, y, 12 + sin(time() * 0.5) * 3, 9)
  print("SOLARBOX", 46, 4, 7)
  print("ARROWS + Z", 40, 120, 13)
end
