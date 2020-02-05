#version 100

precision highp float;

attribute vec3 position;
attribute vec3 color;
attribute vec4 instanceColor;
attribute vec4 instanceColorEdge;
attribute mat4 instancedModelView;

uniform mat4 projection;
uniform mat4 modelView;
uniform mat4 viewMatrix;

/* not used */
uniform vec4 lightColor;
uniform vec4 lightDirection;

varying vec4 vColor;

void main(void) {
    vec4 pos4 = vec4(position, 1.0);
    gl_Position = projection * viewMatrix * modelView * instancedModelView * pos4;
    if (color.x < -1.0) {
        vColor = instanceColorEdge;
    } else if (color.x < 0.0) {
        vColor = instanceColor;
    } else {
        vColor = vec4(color, 1.0);
    }
}
