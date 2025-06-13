local rb = require('rootbeer')
-- rb.debug_test('Hello') -- YIPPEE!

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

require('tale.test2')
require('test3')

rb.ref_file('./test.bash')

-- local ok, err = pcall(function()
-- 	assert(rb.link_file('./test.bash', './test2.bash'))
-- end)

-- if not ok then
-- 	print("❌ Test failed:", err)
-- else
-- 	print("✅ link_file succeeded")
-- end


-- rb.ref_file('./noexist.bash')
--

-- Create a random table for lua to json
local json_data = rb.to_json({
	test = "foo",
	bar = "joe"
})

print("JSON Data: " .. json_data)
rb.write_file("./test/test.json", json_data)

-- Example of what interpolation can look like
local bash_conf = {
	env = {
		FOO = "bar",
		BAZ = "qux"
	},

	aliases = {
		alias1 = "echo 'This is alias 1'",
		alias2 = "echo 'This is alias 2'"
	}
}

-- Creates a .bashrc
function create_bash_config(config)
	local bashrc = ""

	-- Add environment variables
	if config.env then
		for key, value in pairs(config.env) do
			bashrc = bashrc .. "export " .. key .. "=\"" .. value .. "\"\n"
		end
	end

	-- Add aliases
	if config.aliases then
		for alias, command in pairs(config.aliases) do
			bashrc = bashrc .. "alias " .. alias .. "=\"" .. command .. "\"\n"
		end
	end

	return bashrc
end

local bash_config = rb.interpolate_table(bash_conf, create_bash_config)
rb.write_file("./test/.bashrc.test", bash_config)
