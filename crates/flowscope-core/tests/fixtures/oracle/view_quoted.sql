-- VIEW: quoted identifiers and mixed case
CREATE VIEW "Test_View_Quoted" AS
SELECT "T"."ID_SUBJECT" AS "Id", "T"."ID_SUBJECTTYPE" AS "Type"
FROM "IDM"."REG_SUBJECT" "T";
