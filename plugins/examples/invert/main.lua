-- Инверсия цветов: canvas.map вызывает функцию для каждой точки выделения (или всего холста)
-- и записывает то, что она вернула. Прозрачность не меняется.
function invert(p)
  local k = p.amount / 100
  canvas.map(function(r, g, b, a)
    return r + (255 - 2 * r) * k, g + (255 - 2 * g) * k, b + (255 - 2 * b) * k
  end)
end
