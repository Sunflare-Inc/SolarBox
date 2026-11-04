-- SolarBox demo cart: arrows move the sun, Z changes its colour.
x, y, col = 64, 64, 10

function _init()
  stars = {}
  for i = 1, 40 do
    add(stars, {x = rnd(128), y = rnd(128), s = 0.1 + rnd(0.5)})
  end
end

-- API reference:
-- btn(n)  - returns true if button n is held down
-- btnp(n) - returns true if button n was pressed this frame
-- rnd(n)  - Returns a random number between 0 and n
-- flr(n)  - Returns the largest integer less than or equal to n
-- sin(n)  - Returns the sine of n (in radians)
-- time()  - Returns the number of seconds since the program started
-- pset(x, y, c) - Sets the pixel at (x, y) to color c
-- circ(x, y, r, c) - Draws a circle outline at (x, y) with radius r and color c
-- circfill(x, y, r, c) - Draws a filled circle at (x, y) with radius r and color c
-- cls(c) - Clears the screen to color c
-- print(s, x, y, c) - Prints string s at (x, y) in color c
-- foreach(t, f) - Calls function f for each element in table t
-- add(t, v) - Adds value v to the end of table t

function movectrl() -- Movement controller, edit as you see fit.
  if btn(0) then x = x - 1 end
  if btn(1) then x = x + 1 end
  if btn(2) then y = y - 1 end
  if btn(3) then y = y + 1 end
end

function _update60()
  movectrl()
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
