print("Testing require from lua/ directory...")

local shells = require("rootbeer.shells")
print("Loaded rootbeer.shells module: " .. type(shells))

local zsh = require("rootbeer.shells.zsh")
print("Loaded rootbeer.shells.zsh module: " .. type(zsh))

print("require test passed!")
