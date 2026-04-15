-- VIEW with unqualified column references (Oracle-specific)
CREATE VIEW dm.v_employee_info AS
SELECT emp_id, salary, department_name
FROM hr.employees e
JOIN hr.departments d ON e.dept_id = d.dept_id;
