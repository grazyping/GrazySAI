-- Своя динамика: функция получает входы отпечатка и возвращает, как его изменить.
-- Входы: pressure (0..1), speed (px за событие), direction (0..1), distance (px от начала мазка),
-- random и random2 (0..1). Выход: size, density, ratio — множители; angle — градусы;
-- scatter — сдвиг поперёк мазка в долях диаметра. Чего нет в ответе — не меняется.

function jitter(i)
  return {
    size = 0.6 + i.random * 0.6,
    angle = (i.random2 - 0.5) * 120,
    scatter = (i.random - 0.5) * 0.8,
  }
end

-- мазок плотнеет с нажимом и растёт к 600-му пикселю пути
function grow(i)
  local t = math.min(i.distance / 600, 1)
  return { size = 0.3 + 0.7 * t, density = 0.4 + 0.6 * i.pressure }
end
