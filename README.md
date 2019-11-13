# mem_shared_data

## How to play with the program

Start the server by compiling the projet

Using netcat connect to the server using the following command : 

`nc 127.0.0.1 8080`

Once connected to set a variable you have to send to the server a string formatted like so :

`set <var_name> <var_value>`

Example :

`set my_var 10`

To get the value of a variable created and shared a string formatted like so have to be sent:

`get <var_name>`

Example : 

`get my_var`