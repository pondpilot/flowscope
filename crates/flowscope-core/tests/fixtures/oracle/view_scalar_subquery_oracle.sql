-- VIEW with scalar subquery (Oracle-specific)
CREATE VIEW dm.v_emp_with_dept AS
SELECT e.emp_id, e.salary,
       (SELECT department_name FROM hr.departments WHERE dept_id = e.dept_id) AS dept_name
FROM hr.employees e;
