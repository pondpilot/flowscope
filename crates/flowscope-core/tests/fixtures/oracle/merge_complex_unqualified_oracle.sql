-- MERGE with unqualified columns and scalar subquery (Oracle-specific)
MERGE INTO dm.employee_dim trg
USING (SELECT emp_id, department_name, job_title,
              base_salary + NVL((SELECT SUM(bonus_amount) FROM pay.bonuses WHERE emp_id = u.emp_id), 0) AS calc_salary
       FROM stg.emp_updates u
       JOIN hr.departments ON u.new_dept_id = department_id
       JOIN hr.jobs ON u.new_job_id = job_id) src
ON (trg.emp_key = src.emp_id)
WHEN MATCHED THEN UPDATE SET trg.dept_name = src.department_name, trg.job_title = src.job_title, trg.current_salary = src.calc_salary
WHEN NOT MATCHED THEN INSERT (emp_key, dept_name, job_title, current_salary) VALUES (src.emp_id, src.department_name, src.job_title, src.calc_salary);
