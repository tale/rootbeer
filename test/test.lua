local rb = require('rootbeer')
rb.debug_test('Hello') -- YIPPEE!

-- Loop through and print the numbers 1 to 10
for i = 1, 10 do
	print('Number: ' .. i)
end

-- Create a table
local table = {
	['key1'] = 'value1',
	['key2'] = 'value2',
	['key3'] = 'value3'
}

-- Loop through the table and print the key and value
for key, value in pairs(table) do
	print('Key: ' .. key .. ', Value: ' .. value)
end
