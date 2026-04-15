-- UPDATE with EXISTS and SELECT * (Oracle-specific)
UPDATE hr.employees e
SET salary = salary * 1.15
WHERE EXISTS (SELECT * FROM hr.departments d
              WHERE e.department_id = d.department_id AND d.department_name = 'Sales');
