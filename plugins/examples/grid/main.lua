-- Сетка: новый слой, линии текущим цветом. canvas.set(x, y, r, g, b, a) ставит одну точку.
function grid(p)
  local w, h = canvas.width(), canvas.height()
  local r, g, b = color.get()
  canvas.new_layer("Сетка")
  for y = 0, h - 1 do
    for x = 0, w - 1 do
      local on = (x % p.step) < p.width or (y % p.step) < p.width
      if p.diagonal >= 0.5 and ((x - y) % p.step) < p.width then on = true end
      if on then canvas.set(x, y, r, g, b, 255) end
    end
  end
  log("сетка", w, "×", h, "шаг", p.step)
end
