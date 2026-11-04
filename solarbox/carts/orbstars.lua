-- SolarBox demo cart: arrows move the sun, Z changes its colour.
px, py, pcol, phea = 96, 96, 10, 100
ex, ey, ecol, ehea = 32, 32, 10, 100
state = "menu"
orbchance = 1 - 0.05
healchance = 1 - 0.02
bombchance = 1 - 0.01
score = 0
hiscore = 0

function _init()
  stars = {}
  orbs = {}
  heals = {}
  bombs = {}
  for i = 1, 40 do
    add(stars, {x = rnd(128), y = rnd(128), s = 0.1 + rnd(0.5)})
  end
end

function _update60()
  foreach(stars, function(s)
    s.x = (s.x - s.s) % 128
  end)
  if state == "menu" then
    if btnp(4) then state = "play" end
  elseif state == "play" then
    if btnp(5) then state = "gameover" end
    if btn(0) then 
      if px > 5 then px = px - 0.5 end
    elseif btn(1) then 
      if px < 123 then px = px + 0.5 end
    end
    if btn(2) then 
      if py > 5 then py = py - 0.5 end
    elseif btn(3) then 
      if py < 123 then py = py + 0.5 end
    end
    if ex < px then ex = ex + 0.25 elseif ex > px then ex = ex - 0.25 end
    if ey < py then ey = ey + 0.25 elseif ey > py then ey = ey - 0.25 end
    if abs(ex - px) <= 4 and abs(ey - py) <= 4 then
      phea = phea - 1
    end
    if phea <= 0 then state = "gameover" end
    if ehea <= 0 then
      ex = rnd(128)
      ey = rnd(128)
      ehea = 100
    end
    pcol = phea // 10
    ecol = ehea // 10

    if rnd(100)/100 > orbchance then
      add(orbs, {x = rnd(128), y = rnd(128), s = 0.1 + rnd(0.5)})
    end
    if rnd(100)/100 > healchance then
      add(heals, {x = rnd(128), y = rnd(128), s = 0.1 + rnd(0.5)})
    end
    if rnd(100)/100 > bombchance then
      add(bombs, {x = rnd(128), y = rnd(128), s = 0.1 + rnd(0.5)})
    end
    for i = #orbs, 1, -1 do
      local orb = orbs[i]
      orb.x = (orb.x - orb.s) % 128
      if abs(orb.x - px) <= 4 and abs(orb.y - py) <= 4 then
        score = score + 1
        del(orbs, orb)
      end
      if abs(orb.x - ex) <= 4 and abs(orb.y - ey) <= 4 then
        score = score - 1
        del(orbs, orb)
      end
    end
    for i = #heals, 1, -1 do
      local heal = heals[i]
      heal.x = (heal.x - heal.s) % 128
      if abs(heal.x - px) <= 4 and abs(heal.y - py) <= 4 then
        phea = min(phea + 10, 100)
        del(heals, heal)
      end
      if abs(heal.x - ex) <= 4 and abs(heal.y - ey) <= 4 then
        ehea = min(ehea + 10, 100)
        del(heals, heal)
      end
    end
    for i = #bombs, 1, -1 do
      local bomb = bombs[i]
      bomb.x = (bomb.x - bomb.s) % 128
      if abs(bomb.x - px) <= 4 and abs(bomb.y - py) <= 4 then
        phea = max(phea - 10, 0)
        del(bombs, bomb)
      end
      if abs(bomb.x - ex) <= 4 and abs(bomb.y - ey) <= 4 then
        ehea = max(ehea - 10, 0)
        del(bombs, bomb)
      end
    end
  elseif state == "gameover" then
    hiscore = max(hiscore, score)
    if btnp(4) then
      phea = 100
      ehea = 100
      px, py = 96, 96
      ex, ey = 32, 32
      score = 0
      state = "menu"
    end
  end
end

function _draw()
  cls(1)
  foreach(stars, function(s) pset(s.x, s.y, 6) end)
  foreach(orbs, function(o) pset(o.x, o.y, 10) end)
  foreach(heals, function(h) pset(h.x, h.y, 11) end)
  foreach(bombs, function(b) pset(b.x, b.y, 8) end)
  if state == "play" then
    circfill(ex, ey, 2, ecol)
    circ(ex, ey, 4 + sin(time() * 0.5) * 2, 9)
    circfill(px, py, 4, pcol)
    circ(px, py, 7 + sin(time() * 0.5) * 3, 9)
    print("ORBSTARS", 48, 4, 7)
    print("ARROWS", 5, 120, 13)
    print("SCORE: " .. score, 5, 10, 7)
    print("HEALTH: " .. phea, 80, 10, 7)
  elseif state == "menu" then
    print("ORBSTARS", 48, 20, 7)
    print("PRESS Z TO PLAY", 35, 100, 7)
    circfill(64, 64, 8, 10)
    circ(64, 64, 12 + sin(time() * 0.5) * 3, 9)
  elseif state == "gameover" then
    print("GAME OVER", 48, 100, 8)
    print("PRESS Z TO RETURN TO MENU", 15, 80, 7)
    print("HISCORE: " .. hiscore, 5, 60, 7)
    circfill(64, 40, 8, 0)
    circ(64, 40, 12 + sin(time() * 0.5) * 3, 15)
  end
end
