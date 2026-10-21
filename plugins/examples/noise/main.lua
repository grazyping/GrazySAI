-- Шум: к каждому каналу прибавляется случайное число от −сила до +сила.
function noise(p)
  local s = p.amount
  canvas.map(function(r, g, b, a)
    if a == 0 then return r, g, b end
    if p.mono >= 0.5 then
      local d = (math.random() * 2 - 1) * s
      return r + d, g + d, b + d
    end
    return r + (math.random() * 2 - 1) * s, g + (math.random() * 2 - 1) * s, b + (math.random() * 2 - 1) * s
  end)
end
