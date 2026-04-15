-- MERGE INTO subquery as target
MERGE INTO (SELECT employee_id, first_name, salary, department_id FROM employees WHERE department_id = 50) e
USING (SELECT employee_id, salary * 1.1 AS new_salary FROM employees1 WHERE department_id = 50 AND hire_date < '2020-01-01') s
ON (e.employee_id = s.employee_id)
WHEN MATCHED THEN UPDATE SET e.salary = s.new_salary;
