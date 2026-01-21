-- PostgreSQL array slicing syntax variations
-- Tests parser handling of array subscript and slice operators

-- Basic array slicing with omitted start/end
SELECT a[:], b[:1], c[2:], d[2:3]
FROM array_data;

-- Array access with expressions in subscripts
SELECT arr[1+2:3+4], arr[5+6]
FROM computed_indices;

-- Multi-dimensional array access
SELECT matrix[1][2], cube[1][2][3]
FROM multidim_arrays;

-- Array slicing in WHERE clause
SELECT id, values
FROM measurements
WHERE values[1:3] = ARRAY[1, 2, 3];

-- Array slicing with negative indices
SELECT data[-2:-1], data[-1]
FROM indexed_data;
