-- INSERT with scalar subquery aggregation (Oracle-specific)
INSERT INTO report.emp_financial_summary (emp_id, dept_name, location_city, base_salary, total_calc_bonus)
SELECT e.emp_id, department_name, city, salary,
       (SELECT SUM(bonus_amount * multiplier)
        FROM pay.bonuses JOIN pay.bonus_types USING (type_id)
        WHERE emp_id = e.emp_id)
FROM hr.employees e
JOIN hr.departments d ON e.department_id = d.department_id
JOIN hr.locations l ON d.location_id = l.location_id;
